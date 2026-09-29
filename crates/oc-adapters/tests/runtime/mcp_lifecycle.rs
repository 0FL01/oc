//! Publication/cleanup risks of the generation-owned async MCP lifecycle.
use super::*;

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
                ["mcp refresh tools-list: transport (retryable=true)"]
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
        Err(RuntimeError::McpAttach {
            safe_code: "catalog_limit",
            ..
        })
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
