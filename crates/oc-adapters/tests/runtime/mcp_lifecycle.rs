//! Publication/cleanup risks of the generation-owned async MCP lifecycle.
use super::*;

async fn provider_request(
    listener: &tokio::net::TcpListener,
) -> (tokio::net::TcpStream, serde_json::Value) {
    use tokio::io::AsyncReadExt as _;
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("owned provider accept deadline")
        .unwrap();
    let mut raw = Vec::new();
    let request = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let mut chunk = [0; 8192];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0 && raw.len() + count <= 1024 * 1024);
            raw.extend_from_slice(&chunk[..count]);
            if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&raw[..end]).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if raw.len() >= end + 4 + length {
                    break serde_json::from_slice(&raw[end + 4..end + 4 + length]).unwrap();
                }
            }
        }
    })
    .await
    .expect("owned provider request deadline");
    (socket, request)
}

pub(super) fn diagnostic_name(name: &str) -> String {
    use sha2::Digest as _;
    format!("server-{:x}", sha2::Sha256::digest(name.as_bytes()))[..63].into()
}

async fn wait_mcp_state(runtime: &Runtime<'_>, name: &str, state: oc_core::queries::McpStatus) {
    use sha2::Digest;
    let digest = format!("{:x}", sha2::Sha256::digest(name.as_bytes()));
    let id = format!("mcp-{}", &digest[..16]);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if runtime
                .mcp_status()
                .servers
                .iter()
                .any(|row| row.id == id && row.status == state)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned typed MCP state deadline");
}

#[tokio::test]
async fn mcp_late_failure_reaches_completed_existing_and_fresh_turn_without_startup_barrier() {
    use oc_core::queries::McpStatus;
    use tokio::io::AsyncWriteExt as _;

    for fresh in [false, true] {
        let mut permissions = allow_all();
        permissions.insert("healthy__ping".into(), Permission::Allow);
        let (harness, mut generation) = make_harness(permissions);
        let failed_gate = harness._project.path().join("release-failure");
        let slow_gate = harness._project.path().join("never-release-slow");
        for (name, gate, failure) in [
            ("healthy", "-".to_string(), false),
            ("slow", slow_gate.display().to_string(), false),
            ("unavailable", failed_gate.display().to_string(), true),
        ] {
            let mut command = vec![
                "/usr/bin/python3".into(),
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../fixtures/mcp10-lifecycle.py"
                )
                .into(),
                harness
                    ._project
                    .path()
                    .join(format!("{name}.json"))
                    .display()
                    .to_string(),
                name.into(),
                gate,
                "-".into(),
                harness._project.path().display().to_string(),
            ];
            if failure {
                command.push("initialize-error".into());
            }
            generation.mcp.insert(
                name.into(),
                McpEntry {
                    kind: "local".into(),
                    enabled: true,
                    command,
                    environment: BTreeMap::from([(
                        "MCP10_CANARY".into(),
                        "mcp10-activated-canary".into(),
                    )]),
                    ..Default::default()
                },
            );
        }
        let runtime = runtime_of(&harness, generation, Vec::new());
        if !fresh {
            runtime.create_session("s").unwrap();
        }
        runtime.start_mcp().unwrap();
        // Wait only for the healthy sibling, never for initial startup as a whole.
        wait_mcp_state(&runtime, "healthy", McpStatus::Connected).await;
        let initial_binding = runtime.mcp_status().binding;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let turn = async {
            let params = params(
                "s",
                "held late failure",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            );
            if fresh {
                runtime
                    .run_fresh_turn_with_tool_events(
                        params,
                        None,
                        |_| {},
                        |_, _| {},
                        |_, _| {},
                        |_, _| {},
                    )
                    .await
            } else {
                runtime.run_turn(params).await
            }
        };
        let barrier = async {
            let (mut socket, request) = provider_request(&listener).await;
            socket.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n: held\n\n").await.unwrap();
            assert!(
                request["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == "healthy__ping")
            );
            assert!(
                request["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|tool| !tool["name"].as_str().unwrap().starts_with("unavailable__"))
            );
            assert!(request.to_string().contains("LIFECYCLE_GUIDANCE"));
            wait_mcp_state(&runtime, "unavailable", McpStatus::Pending).await;
            std::fs::write(&failed_gate, "release-after-actual-provider-request").unwrap();
            wait_mcp_state(&runtime, "unavailable", McpStatus::Failed).await;
            let snapshot = runtime.mcp_status();
            assert_eq!(snapshot.binding, initial_binding);
            assert!(
                snapshot
                    .servers
                    .iter()
                    .any(|row| row.name == diagnostic_name("slow")
                        && row.status == McpStatus::Pending)
            );
            let failed = snapshot
                .servers
                .iter()
                .find(|row| row.name == diagnostic_name("unavailable"))
                .unwrap();
            assert_eq!(failed.tools, 0);
            assert_eq!(
                failed.diagnostic.as_ref().unwrap().code,
                oc_core::queries::ServiceCode::Transport
            );
            socket
                .write_all(
                    (sse_delta("completed despite late failure") + &sse_completed()).as_bytes(),
                )
                .await
                .unwrap();
            request
        };
        let (report, original_request) = tokio::join!(turn, barrier);
        let report = report.unwrap();
        // A next request uses the healthy publication while slow is STILL held.
        let next = runtime.run_turn(params(
            "s",
            "next healthy turn",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ));
        let respond = async {
            let (mut socket, request) = provider_request(&listener).await;
            assert_eq!(request["tools"], original_request["tools"]);
            assert!(!request.to_string().contains("PRIVATE_LATE_MCP_FAILURE"));
            socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}", sse_delta("next healthy") + &sse_completed()).as_bytes()).await.unwrap();
        };
        let (next, ()) = tokio::join!(next, respond);
        let next = next.unwrap();
        runtime.shutdown_mcp().await.unwrap();
        for name in ["healthy", "slow", "unavailable"] {
            let counts: serde_json::Value = serde_json::from_slice(
                &std::fs::read(harness._project.path().join(format!("{name}.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(counts["closed"], 1);
            // SAFETY: signal zero probes only the PID recorded by this owned fixture.
            let alive = unsafe { libc::kill(counts["pid"].as_i64().unwrap() as libc::pid_t, 0) };
            assert_ne!(alive, 0, "owned {name} process not reaped");
            if name == "unavailable" {
                assert_eq!(
                    (
                        counts["failed"].as_u64(),
                        counts["catalog"].as_u64(),
                        counts["call"].as_u64()
                    ),
                    (Some(1), Some(0), Some(0))
                );
            }
        }
        assert_eq!(report.status, TurnStatus::Completed);
        assert_eq!(next.status, TurnStatus::Completed);
        let expected = format!(
            "mcp {} initialize: transport (retryable=true)",
            diagnostic_name("unavailable")
        );
        assert_eq!(
            report.warnings.as_slice(),
            std::slice::from_ref(&expected),
            "fresh={fresh}: late typed degradation lost"
        );
        assert_eq!(next.warnings, [expected]);
        assert_eq!(report.service_warning_range, 0..report.warnings.len());
        assert_eq!(next.service_warning_range, 0..next.warnings.len());
        assert_eq!(harness.db.history_len("s").unwrap(), 4);
    }
}

#[tokio::test]
async fn mcp07_failed_relist_retains_ready_catalog_and_retries_at_next_turn_only() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("refresh.py");
    let log = harness._project.path().join("refresh.log");
    std::fs::write(&script, r#"import json, sys
count = 0
with open(sys.argv[1], 'a') as f: f.write('spawn\n')
for line in sys.stdin:
    r = json.loads(line)
    method = r['method']
    if method == 'initialize':
        result = {'protocolVersion':'2025-11-25','capabilities':{'tools':{'listChanged':True}},'serverInfo':{'name':'refresh','version':'fixture'}}
    elif method == 'tools/list':
        count += 1
        with open(sys.argv[1], 'a') as f: f.write('list\n')
        if count == 2:
            print(json.dumps({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32603,'message':'PRIVATE_MCP07_ERROR'}}), flush=True)
            continue
        if count == 1:
            print(json.dumps({'jsonrpc':'2.0','method':'notifications/tools/list_changed'}), flush=True)
        result = {'tools':[{'name':'old' if count == 1 else 'new','inputSchema':{'type':'object'}}]}
    else:
        continue
    print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#).unwrap();
    generation.mcp.insert(
        "refresh".into(),
        McpEntry {
            kind: "local".into(),
            enabled: true,
            command: vec![
                "/usr/bin/python3".into(),
                script.display().to_string(),
                log.display().to_string(),
            ],
            timeout: Some(1000),
            ..Default::default()
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let (base, _, requests) =
        Fake::start_recording(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    for prompt in ["old retained", "repair at boundary", "no further relist"] {
        let report = runtime
            .run_turn(params(
                "s",
                prompt,
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        if prompt == "old retained" {
            assert_eq!(
                report.warnings,
                [format!(
                    "mcp {} tools-list: transport (retryable=true)",
                    diagnostic_name("refresh")
                )]
            );
            let row = &runtime.mcp_status().servers[0];
            assert_eq!(row.status, oc_core::queries::McpStatus::Connected);
            assert_eq!(row.tools, 1);
        }
    }
    {
        let requests = requests.lock().unwrap();
        for (index, tool) in [
            (0, "refresh__old"),
            (1, "refresh__new"),
            (2, "refresh__new"),
        ] {
            assert!(
                requests[index]["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["name"] == tool)
            );
            assert!(!requests[index].to_string().contains("PRIVATE_MCP07_ERROR"));
        }
    }
    let counters = std::fs::read_to_string(&log).unwrap();
    assert_eq!(counters.lines().filter(|line| *line == "spawn").count(), 1);
    assert_eq!(counters.lines().filter(|line| *line == "list").count(), 3);
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn mcp10_async_generation_cap_is_fatal_and_reaps_every_connected_peer() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("caps.py");
    std::fs::write(&script, r#"import json, os, sys
with open(sys.argv[1], 'w') as f: f.write(str(os.getpid()))
for line in sys.stdin:
    r=json.loads(line)
    if r['method'] == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'caps','version':'fixture'}}
    elif r['method'] == 'tools/list':
        result={'tools':[{'name':'tool_%d' % n,'inputSchema':{'type':'object'}} for n in range(20)]}
        with open(sys.argv[1]+'.listed','w') as f: f.write('1')
    else: continue
    print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#).unwrap();
    let pids: Vec<_> = (0..7)
        .map(|n| harness._project.path().join(format!("peer_{n}.pid")))
        .collect();
    for (n, pid) in pids.iter().enumerate() {
        generation.mcp.insert(
            format!("peer_{n}"),
            McpEntry {
                kind: "local".into(),
                enabled: true,
                command: vec![
                    "/usr/bin/python3".into(),
                    script.display().to_string(),
                    pid.display().to_string(),
                ],
                timeout: Some(1000),
                ..Default::default()
            },
        );
    }
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    runtime.start_mcp().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !pids
        .iter()
        .all(|pid| std::path::PathBuf::from(format!("{}.listed", pid.display())).exists())
    {
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // The fatal publication is observed through the ordinary turn/shutdown
    // contracts; none of the partially admitted schemas reach a provider.
    while runtime
        .mcp_status()
        .servers
        .iter()
        .any(|row| row.status != oc_core::queries::McpStatus::Disabled)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "fatal cap did not retire all resources"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (base, hits) = Fake::start(
        vec![sse_delta("forbidden") + &sse_completed()],
        Duration::ZERO,
    );
    assert!(matches!(
        runtime
            .run_turn(params(
                "s",
                "no partial catalog",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await,
        Err(RuntimeError::McpCatalogLimit)
    ));
    assert!(
        runtime.shutdown_mcp().await.is_err(),
        "fatal cap became clean success"
    );
    assert_eq!(*hits.lock().unwrap(), 0);
    assert!(harness.db.read_history("s").unwrap().is_empty());
    for pid in pids {
        let pid = std::fs::read_to_string(pid)
            .unwrap()
            .parse::<libc::pid_t>()
            .unwrap();
        // SAFETY: signal zero only probes a PID written by this fixture.
        let alive = unsafe { libc::kill(pid, 0) };
        assert_ne!(alive, 0, "fatal async cap left an owned child");
    }
}
