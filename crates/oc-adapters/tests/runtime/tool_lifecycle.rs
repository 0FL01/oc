//! Permissions, durable effects and generation-owned MCP/tool lifecycle.

use super::*;

#[tokio::test]
async fn runtime_shell_timeout_and_cancellation_have_typed_terminal_states() {
    for cancelled in [false, true] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("shell-state").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let marker = harness._project.path().join("marker");
        let cancellation = if cancelled {
            let flag = cancel.clone();
            Some(std::thread::spawn(move || {
                let started = std::time::Instant::now();
                while !marker.exists() && started.elapsed() < Duration::from_secs(5) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                flag.store(true, Ordering::Release);
            }))
        } else {
            None
        };
        let batch = sse_tool_call(
            "effect",
            "bash",
            &serde_json::json!({"argv":["/bin/sh","-c","printf partial > marker; sleep 5"],"timeout_ms":if cancelled {5000} else {100}}),
        ) + &sse_completed();
        let (base, _, _) = Fake::start_recording(
            vec![batch, sse_delta("done") + &sse_completed()],
            Duration::ZERO,
        );
        let report = runtime
            .run_turn(params(
                "shell-state",
                "write then interrupt",
                &harness,
                provider_of(&base),
                &cancel,
            ))
            .await
            .unwrap();
        if let Some(thread) = cancellation {
            thread.join().unwrap();
        }
        let expected = if cancelled { "cancelled" } else { "timed_out" };
        assert_eq!(report.calls[0].state, expected);
        assert_eq!(
            report.status,
            if cancelled {
                TurnStatus::Cancelled
            } else {
                TurnStatus::Completed
            }
        );
        let operation = harness.db.list_tool_ops("shell-state").unwrap().remove(0);
        assert_eq!(operation.state, expected);
        assert_eq!(
            std::fs::read(harness._project.path().join("marker")).unwrap(),
            b"partial"
        );
    }
}

/// Real runtime/provider/MCP path: dropping a polled tools/call future must
/// remain quarantined even if the caller reloads before starting a new turn.
#[tokio::test]
async fn v07b_dropped_remote_call_reload_keeps_unknown_and_refuses_retry() {
    dropped_remote_call_cannot_retry_after(false).await;
}

#[tokio::test]
async fn v07b_dropped_remote_call_shutdown_keeps_unknown_and_refuses_retry() {
    dropped_remote_call_cannot_retry_after(true).await;
}

#[tokio::test]
async fn resource_permissions_gate_real_dispatch_before_side_effects() {
    let (harness, mut generation) = make_harness(allow_all());
    generation.permission_rules =
        oc_adapters::permissions::PermissionRules::from_config(&serde_json::json!({
            "permission": {
                "read": {"*":"deny", "safe/*":"allow", "safe/secret*":"deny"},
                "edit": {"safe/*":"allow", "safe/secret*":"deny"},
                "bash": {"/bin/echo ok":"allow"}
            }
        }))
        .unwrap();
    std::fs::create_dir(harness._project.path().join("safe")).unwrap();
    std::fs::write(
        harness._project.path().join("safe/input"),
        "allowed content",
    )
    .unwrap();
    std::fs::write(harness._project.path().join("private"), "private canary").unwrap();
    std::fs::write(
        harness._project.path().join("safe/secret*keys"),
        "star canary\n",
    )
    .unwrap();
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("resources").unwrap();
    let (events, _) = tokio::sync::broadcast::channel(32);
    runtime.set_approval_events(&events);
    runtime.register_approval_consumer(false);
    let calls = [
        ("read-ok", "read", serde_json::json!({"path": "safe/input"})),
        (
            "read-no",
            "read",
            serde_json::json!({"path":"safe/../private"}),
        ),
        (
            "patch-ok",
            "apply_patch",
            serde_json::json!({"patchText":"*** Begin Patch\n*** Add File: safe/new\n+allowed\n*** End Patch"}),
        ),
        (
            "patch-no",
            "apply_patch",
            serde_json::json!({"patchText":"*** Begin Patch\n*** Update File: safe/input\n*** Move to: escaped\n@@\n-allowed content\n+changed\n*** End Patch"}),
        ),
        (
            "bash-ok",
            "bash",
            serde_json::json!({"argv":["/bin/echo","ok"]}),
        ),
        (
            "bash-no",
            "bash",
            serde_json::json!({"argv":["/bin/touch","marker"]}),
        ),
        (
            "read-star",
            "read",
            serde_json::json!({"path":"safe/secret*keys"}),
        ),
        (
            "patch-star",
            "apply_patch",
            serde_json::json!({"patchText":"*** Begin Patch\n*** Delete File: safe/secret*keys\n*** End Patch"}),
        ),
    ];
    let script = calls
        .iter()
        .map(|(id, tool, args)| sse_tool_call(id, tool, args))
        .collect::<String>()
        + &sse_completed();
    let (base, _, requests) = Fake::start_recording(
        vec![script, sse_delta("done") + &sse_completed()],
        Duration::ZERO,
    );
    let running = runtime.run_turn(params(
        "resources",
        "tools",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    ));
    let rejecting = async {
        for _ in 0..2 {
            let request = approval_lifecycle::next_request(&runtime).await;
            runtime
                .reply_approval(oc_core::approval::ApprovalReply {
                    id: request.id,
                    binding: request.binding,
                    decision: oc_core::approval::ApprovalDecision::Reject {
                        feedback: Some("approval required; do not execute this call".into()),
                    },
                })
                .unwrap();
        }
    };
    let (report, ()) = tokio::join!(running, rejecting);
    let report = report.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        report
            .calls
            .iter()
            .map(|call| call.state.as_str())
            .collect::<Vec<_>>(),
        [
            "completed",
            "failed",
            "completed",
            "denied",
            "completed",
            "denied",
            "failed",
            "failed"
        ],
        "{:?}",
        report.calls
    );
    assert!(report.calls[3].output.contains("approval required"));
    assert!(report.calls[5].output.contains("approval required"));
    assert_eq!(report.calls[6].output, "error: denied read");
    assert_eq!(report.calls[7].output, "error: denied apply_patch");
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("safe/secret*keys")).unwrap(),
        "star canary\n"
    );
    assert!(
        !requests.lock().unwrap()[1]
            .to_string()
            .contains("star canary")
    );
    assert!(harness._project.path().join("safe/new").exists());
    assert!(!harness._project.path().join("escaped").exists());
    assert!(!harness._project.path().join("marker").exists());
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("safe/input")).unwrap(),
        "allowed content"
    );
    assert!(
        !requests.lock().unwrap()[1]
            .to_string()
            .contains("private canary")
    );
}

#[tokio::test]
async fn aud06_intent_failure_prevents_patch() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_intent BEFORE INSERT ON tool_operations BEGIN SELECT RAISE(ABORT, 'injected intent failure'); END;").unwrap();
    let tool = sse_tool_call(
        "call-patch",
        "apply_patch",
        &serde_json::json!({"patchText": "*** Begin Patch\n*** Add File: sentinel\n+must not exist\n*** End Patch"}),
    );
    let (base, _) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
    let mut events = Vec::new();
    let result = runtime
        .run_turn_with_tool_events(
            params("s", "patch", &harness, provider_of(&base), &NO_CANCEL),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| events.push(event.clone()),
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert!(
        !harness._project.path().join("sentinel").exists(),
        "mutation ran before durable intent"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        ToolCallEvent::ArgumentStream(oc_core::tool_stream::ToolStreamEvent::Pending { .. })
    )));
    assert!(!events.iter().any(|event| matches!(
        event,
        ToolCallEvent::Started { .. }
            | ToolCallEvent::ArgumentStream(oc_core::tool_stream::ToolStreamEvent::Linked { .. })
    )));
    assert!(matches!(
        events.last(),
        Some(ToolCallEvent::ArgumentStream(
            oc_core::tool_stream::ToolStreamEvent::Clear { round: 1 }
        ))
    ));
}

#[tokio::test]
async fn s08_bash_intent_store_fault_prevents_effect_and_recovers_unknown_turn() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("intent-fault").unwrap();
    let marker = harness._project.path().join("marker");
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER fail_bash_intent BEFORE INSERT ON tool_operations
         WHEN NEW.name = 'bash' AND NEW.session_id = 'intent-fault'
         BEGIN SELECT RAISE(ABORT, 'injected bash intent failure'); END;",
    )
    .unwrap();
    let tool = sse_tool_call(
        "touch-marker",
        "bash",
        &serde_json::json!({"argv": ["/bin/touch", "marker"]}),
    );
    let (base, hits) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
    let mut accepted = None;
    let mut tool_events = Vec::new();
    let result = runtime
        .run_turn_with_tool_events(
            params(
                "intent-fault",
                "create marker",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |id| accepted = Some(id.to_string()),
            |_, _| {},
            |_, _| {},
            |_, event| tool_events.push(event.clone()),
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert_eq!(
        *hits.lock().unwrap(),
        1,
        "provider tool call was not received"
    );
    assert!(
        accepted.is_some(),
        "turn was rejected before the injected fault"
    );
    assert!(
        tool_events.iter().all(|event| matches!(
            event,
            ToolCallEvent::ArgumentStream(
                oc_core::tool_stream::ToolStreamEvent::Pending { .. }
                    | oc_core::tool_stream::ToolStreamEvent::Clear { .. }
            )
        )),
        "undurable tool was shown as started"
    );
    assert!(!marker.exists(), "bash ran despite rejected durable intent");

    sql.execute_batch("DROP TRIGGER fail_bash_intent").unwrap();
    drop(sql);
    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    assert_eq!(reopened.recover_interrupted_tools().unwrap(), 0);
    assert!(reopened.list_tool_ops("intent-fault").unwrap().is_empty());
    assert_eq!(
        reopened.read_history("intent-fault").unwrap(),
        [("user".to_string(), "create marker".to_string())]
    );
    let sql = rusqlite::Connection::open(reopened.root().join("oc.sqlite")).unwrap();
    let (status, result): (String, Option<String>) = sql
        .query_row(
            "SELECT status, result FROM turns WHERE id = ?1",
            [accepted.unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        status, "unknown",
        "failed store write must not become success"
    );
    let journal: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(
        journal["display_parts"].as_array().unwrap().len(),
        0,
        "rolled-back intent must not leave a replayable tool card: {journal}"
    );
    let unknown_events: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE session_id = 'intent-fault' AND kind = 'turn_unknown'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unknown_events, 1, "restart must expose an uncertain turn");
    assert!(!marker.exists(), "recovery replayed an undurable effect");
}

#[tokio::test]
async fn aud07_mixed_order_and_mcp_storage_failures() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("mcp.py");
    let order = harness._project.path().join("order");
    std::fs::write(&script, r#"import json, sys
for line in sys.stdin:
    r = json.loads(line)
    method = r.get('method')
    if method == 'initialize':
        result = {'protocolVersion': '2025-11-25', 'capabilities': {'tools': {}}, 'serverInfo': {'name': 'fixture', 'version': '1'}}
    elif method == 'tools/list':
        result = {'tools': [{'name': 'mark', 'description': 'mark', 'inputSchema': {'type': 'object'}}]}
    elif method == 'tools/call':
        with open(sys.argv[1], 'a') as f: f.write('M\n')
        result = {'content': [{'type': 'text', 'text': 'marked'}], 'isError': False}
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': r['id'], 'result': result}), flush=True)
"#).unwrap();
    generation
        .permissions
        .insert("fixture__mark".to_string(), Permission::Allow);
    generation.mcp.insert(
        "fixture".to_string(),
        McpEntry {
            kind: "local".to_string(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".to_string(),
                script.to_string_lossy().into_owned(),
                order.to_string_lossy().into_owned(),
            ],
            timeout: Some(2000),
            codemode: None,
            ..Default::default()
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let first = sse_tool_call(
        "builtin-first",
        "bash",
        &serde_json::json!({"argv": ["/bin/sh", "-c", "printf 'B1\\n' >> order"]}),
    );
    let middle = sse_tool_call("mcp-middle", "fixture__mark", &serde_json::json!({}));
    let last = sse_tool_call(
        "builtin-last",
        "bash",
        &serde_json::json!({"argv": ["/bin/sh", "-c", "printf 'B2\\n' >> order"]}),
    );
    let batch = first + &middle + &last + &sse_completed();
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    for fault in ["", "intent", "outcome"] {
        let (base, _) = Fake::start(vec![batch.clone(), sse_completed()], Duration::ZERO);
        std::fs::write(&order, "").unwrap();
        match fault {
            "intent" => sql.execute_batch("CREATE TRIGGER fail_mcp BEFORE INSERT ON tool_operations WHEN NEW.name = 'fixture__mark' BEGIN SELECT RAISE(ABORT, 'injected MCP intent'); END;").unwrap(),
            "outcome" => sql.execute_batch("CREATE TRIGGER fail_mcp BEFORE UPDATE ON tool_operations WHEN NEW.name = 'fixture__mark' BEGIN SELECT RAISE(ABORT, 'injected MCP outcome'); END;").unwrap(),
            _ => {},
        }
        let p = params(
            "s",
            "ordered tools",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        );
        let result = runtime.run_turn(p).await;
        if fault.is_empty() {
            let report = result.unwrap();
            assert_eq!(
                report
                    .calls
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                ["bash", "fixture__mark", "bash"]
            );
            assert_eq!(std::fs::read_to_string(&order).unwrap(), "B1\nM\nB2\n");
            let ops = harness.db.list_tool_ops("s").unwrap();
            assert!(ops[1].op.ends_with("mcp-middle"));
            assert_eq!(ops[1].turn.as_deref(), Some(report.turn_id.as_str()));
        } else {
            assert_eq!(
                result.unwrap_err(),
                oc_adapters::runtime::RuntimeError::Storage
            );
            assert_eq!(
                std::fs::read_to_string(&order).unwrap(),
                if fault == "intent" { "B1\n" } else { "B1\nM\n" }
            );
            sql.execute_batch("DROP TRIGGER fail_mcp").unwrap();
        }
    }
}

#[tokio::test]
async fn aud11_incomplete_call_never_executes() {
    for terminal in [
        "".to_string(),
        "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\"}}\n\n"
            .to_string(),
        "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\"}}\n\n".to_string(),
        sse_completed(),
    ] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").unwrap();
        let args = serde_json::json!({"argv": ["/bin/sh", "-c", "printf unexpected >> sentinel"]});
        let added = serde_json::json!({"type": "response.output_item.added", "item": {
            "type": "function_call", "id": "fc_partial", "call_id": "call_partial",
            "name": "bash", "arguments": "", "status": "in_progress"
        }});
        let delta = serde_json::json!({"type": "response.function_call_arguments.delta",
            "item_id": "fc_partial", "delta": args.to_string()});
        // Even valid JSON arguments cannot substitute for output_item.done.
        let (base, hits) = Fake::start(
            vec![
                format!("data: {added}\n\ndata: {delta}\n\n{terminal}"),
                "data: {\"type\":\"error\",\"error\":{\"code\":\"insufficient_quota\"}}\n\n".into(),
            ],
            Duration::ZERO,
        );
        let report = runtime
            .run_turn(params("s", "run", &harness, provider_of(&base), &NO_CANCEL))
            .await
            .unwrap();
        assert_ne!(report.status, TurnStatus::Completed, "{terminal}");
        assert!(report.calls.is_empty(), "unfinished batch executed");
        assert!(harness.db.list_tool_ops("s").unwrap().is_empty());
        assert!(!harness._project.path().join("sentinel").exists());
        assert_ne!(
            harness.db.turn_result(&report.turn_id).unwrap().0,
            "completed"
        );
        assert_eq!(
            *hits.lock().unwrap(),
            if terminal == sse_completed() { 1 } else { 2 },
            "real incomplete failures continue; completed-with-unfulfilled-call is local and never admits tools"
        );
    }
}

#[tokio::test]
async fn tool_rounds_execute_and_record() {
    let (harness, generation) = make_harness(allow_all());
    std::fs::write(harness._project.path().join("note.txt"), "file-bytes").expect("seed");
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let tool = sse_tool_call("i1", "read", &serde_json::json!({"path": "note.txt"}));
    let (base, _) = Fake::start(
        vec![
            tool + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );

    let report = runtime
        .run_turn(params(
            "s",
            "read it",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.rounds, 2);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].name, "read");
    assert_eq!(report.calls[0].state, "completed");
    assert!(
        report.calls[0].output.contains("file-bytes"),
        "{}",
        report.calls[0].output
    );

    let ops = harness.db.list_tool_ops("s").expect("ops");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].state, "completed");
}

#[tokio::test]
async fn denied_and_ask_tools_fail_visibly() {
    for permission in [Permission::Deny, Permission::Ask] {
        let mut permissions = allow_all();
        permissions.insert("bash".to_string(), permission);
        let (harness, generation) = make_harness(permissions);
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").expect("create");
        let tool = sse_tool_call("i1", "bash", &serde_json::json!({"argv": ["echo", "x"]}));
        let (base, _) = Fake::start(
            vec![tool + &sse_completed(), sse_completed()],
            Duration::ZERO,
        );
        let result = runtime
            .run_turn(params("s", "run", &harness, provider_of(&base), &NO_CANCEL))
            .await;
        if permission == Permission::Ask {
            assert!(matches!(result, Err(RuntimeError::ApprovalRequired { .. })));
            assert!(harness.db.list_tool_ops("s").unwrap().is_empty());
            continue;
        }
        let report = result.expect("turn");
        assert_eq!(report.calls[0].state, "failed");
        assert!(
            report.calls[0].output.contains("denied"),
            "{}",
            report.calls[0].output
        );
        let ops = harness.db.list_tool_ops("s").expect("ops");
        assert_eq!(ops[0].state, "failed");
    }
}

#[tokio::test]
async fn protected_patch_never_reaches_disk() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, vec!["*.secret".to_string()]);
    runtime.create_session("s").expect("create");
    let patch = "*** Begin Patch\n*** Add File: x.secret\n+boe\n*** End Patch\n";
    let tool = sse_tool_call(
        "i1",
        "apply_patch",
        &serde_json::json!({"patchText": patch}),
    );
    let (base, _) = Fake::start(
        vec![tool + &sse_completed(), sse_completed()],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "s",
            "patch",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.calls[0].state, "failed");
    assert!(
        report.calls[0].output.contains("protected"),
        "{}",
        report.calls[0].output
    );
    assert!(
        !harness._project.path().join("x.secret").exists(),
        "legacy deny wins"
    );
}

#[tokio::test]
async fn aud12_cancel_during_mcp_initialize_reaps_child_before_acceptance() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("stall_initialize.py");
    let pid_file = harness._project.path().join("mcp.pid");
    std::fs::write(
        &script,
        r#"import json, os, sys, time
lists = 0
for line in sys.stdin:
    request = json.loads(line)
    method = request['method']
    if method == 'initialize':
        result = {'protocolVersion':'2025-11-25','capabilities':{'tools':{'listChanged':True}},'serverInfo':{'name':'fixture','version':'1'}}
    elif method == 'tools/list':
        lists += 1
        if lists > 1:
            with open(sys.argv[1], 'w') as f: f.write(str(os.getpid()))
            time.sleep(30)
        else:
            print(json.dumps({'jsonrpc':'2.0','method':'notifications/tools/list_changed'}), flush=True)
        result = {'tools':[]}
    else:
        continue
    print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "stall".to_string(),
        McpEntry {
            kind: "local".to_string(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".to_string(),
                script.to_string_lossy().into_owned(),
                pid_file.to_string_lossy().into_owned(),
            ],
            timeout: Some(30_000),
            codemode: None,
            ..Default::default()
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, hits) = Fake::start(
        vec![sse_delta("unexpected") + &sse_completed()],
        Duration::ZERO,
    );
    let cancel = AtomicBool::new(false);
    let accepted = AtomicBool::new(false);
    // R7 initializes independently; preserve AUD12's real pre-admission
    // cancellation/reap contract using MCP07's held catalog preparation.
    wait_initial_mcp(&runtime).await;
    let outcome = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            async {
                // Synchronize on the held catalog RPC, not a startup delay.
                loop {
                    if std::fs::read_to_string(&pid_file)
                        .is_ok_and(|text| text.parse::<u32>().is_ok())
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let at = std::time::Instant::now();
                cancel.store(true, Ordering::Relaxed);
                at
            },
            runtime.run_turn_with_events(
                params("s", "cancel attach", &harness, provider_of(&base), &cancel),
                |_| {
                    accepted.store(true, Ordering::Relaxed);
                },
                |_, _| panic!("provider started before MCP attached"),
                |_, _| {},
            )
        )
    })
    .await;
    let pid: u32 = std::fs::read_to_string(&pid_file)
        .expect("child received initialize")
        .parse()
        .unwrap();
    let process = std::path::PathBuf::from(format!("/proc/{pid}"));
    let cleanup = tokio::time::timeout(Duration::from_secs(1), async {
        while process.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    if cleanup.is_err() {
        // A failing regression must not leave this fixture running for 30 seconds.
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
    let (cancelled_at, result) = outcome.expect("attach ignored cancellation for five seconds");
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(1),
        "cancel/child cleanup exceeded one second"
    );
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Cancelled
    );
    assert!(cleanup.is_ok(), "stdio child {pid} survived cancellation");
    assert!(!accepted.load(Ordering::Relaxed));
    assert_eq!(*hits.lock().unwrap(), 0);
    assert!(harness.db.read_history("s").unwrap().is_empty());
    assert!(harness.db.list_tool_ops("s").unwrap().is_empty());
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let turns: i64 = sql
        .query_row("SELECT COUNT(*) FROM turns", [], |row| row.get(0))
        .unwrap();
    assert_eq!(turns, 0, "cancelled attach must not begin a turn");
    let events: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind != 'session_created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(events, 0, "cancelled attach must not acknowledge input");
}

#[tokio::test]
async fn aud23_generation_reuse_then_reload_disable_reaps_the_single_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("generation_mcp.py");
    let lifecycle = harness._project.path().join("generation.log");
    std::fs::write(
        &script,
        r#"import json, os, sys
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r = json.loads(line); method = r.get('method'); result = None
    if method == 'initialize':
        result = {'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'generation','version':'1'}}
    elif method == 'tools/list':
        result = {'tools':[{'name':'ping','description':'ping','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "counted".into(),
        McpEntry {
            kind: "local".into(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".into(),
                script.to_string_lossy().into_owned(),
                lifecycle.to_string_lossy().into_owned(),
            ],
            timeout: Some(2_000),
            codemode: None,
            ..Default::default()
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    for prompt in ["first", "second"] {
        assert_eq!(
            runtime
                .run_turn(params(
                    "s",
                    prompt,
                    &harness,
                    provider_of(&base),
                    &NO_CANCEL,
                ))
                .await
                .unwrap()
                .status,
            TurnStatus::Completed
        );
    }
    let log = std::fs::read_to_string(&lifecycle).unwrap();
    let pids = log
        .lines()
        .filter_map(|line| line.strip_prefix("spawn "))
        .map(|pid| pid.parse::<libc::pid_t>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(pids.len(), 1, "one child per generation: {log}");
    // SAFETY: signal 0 only probes the fixture child recorded by that child.
    assert_eq!(unsafe { libc::kill(pids[0], 0) }, 0);

    runtime
        .reload(Generation {
            tool_output: Default::default(),
            compaction: Default::default(),
            config_diagnostics: Vec::new(),
            animations: None,
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions: allow_all(),
            permission_rules: Default::default(),
            provenance: BTreeMap::new(),
            warnings: Vec::new(),
        })
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // SAFETY: signal 0 only probes the fixture child pid.
    while unsafe { libc::kill(pids[0], 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // SAFETY: final liveness probe only.
    let alive = unsafe { libc::kill(pids[0], 0) };
    assert_ne!(alive, 0, "reload left MCP child");
    assert_eq!(
        runtime
            .run_turn(params(
                "s",
                "disabled",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap()
            .status,
        TurnStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(&lifecycle)
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("spawn "))
            .count(),
        1,
        "disabled generation respawned the server"
    );
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn backend_parity_mcp_projection_permissions_history_and_error_canaries() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("parity.py");
    std::fs::write(&script, r#"import json, sys
server = sys.argv[1]
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'},'instructions':'GUIDANCE_'+server.upper()+' use query; PROVIDER-CANARY HEADER-CANARY'}
    elif method == 'tools/list':
        if server == 'broken':
            print(json.dumps({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32603,'message':'UNKNOWN-CANARY'}}),flush=True)
            continue
        result={'tools':[] if server == 'empty' else [{'name':'query','inputSchema':{'type':'object','properties':{'fail':{'type':'boolean'},'throttle':{'type':'boolean'}}}}]}
    elif method == 'tools/call':
        if r['params']['arguments'].get('throttle'):
            result={'isError':True,'structuredContent':{'error':{'code':'RATE_LIMITED'}},'content':[{'type':'text','text':'UNKNOWN-CANARY'}]}
        elif r['params']['arguments'].get('fail'):
            result={'isError':True,'structuredContent':{'error':{'code':'INVALID_ARGUMENTS'}},'content':[{'type':'text','text':'invalid argument: missing parameter query; UNKNOWN-CANARY PROVIDER-CANARY HEADER-CANARY'}]}
        else:
            result={'content':[{'type':'text','text':'Useful explanation '+sys.argv[2]},{'type':'resource','resource':{'uri':'file:///fixture','text':'Embedded content'}}],'structuredContent':{'answer':42,'echo':'PROVIDER-CANARY HEADER-CANARY','argvEcho':sys.argv[2],'token':'UNKNOWN-CANARY'}}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#).unwrap();
    for (server, permission) in [
        ("good", Permission::Allow),
        ("denied", Permission::Deny),
        ("ask", Permission::Ask),
        ("empty", Permission::Allow),
        ("broken", Permission::Allow),
        ("disabled", Permission::Allow),
        ("unlisted", Permission::Deny),
    ] {
        generation.mcp.insert(
            server.into(),
            McpEntry {
                kind: "local".into(),
                enabled: server != "disabled",
                command: vec![
                    "/usr/bin/python3".into(),
                    script.to_string_lossy().into_owned(),
                    server.into(),
                    "CONFIG-CANARY".into(),
                ],
                headers: BTreeMap::from([("x-fixture".into(), "HEADER-CANARY".into())]),
                timeout: Some(2_000),
                ..McpEntry::default()
            },
        );
        if server != "unlisted" {
            generation
                .permissions
                .insert(format!("{server}__query"), permission);
        }
    }
    generation.providers.insert(
        "test".into(),
        oc_adapters::config::ProviderEntry {
            npm: None,
            name: None,
            models: BTreeMap::new(),
            options: oc_adapters::config::ProviderOptions {
                api_key: "PROVIDER-CANARY".into(),
                ..Default::default()
            },
        },
    );
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call("data", "good__query", &serde_json::json!({}))
                + &sse_tool_call("failed", "good__query", &serde_json::json!({"fail":true}))
                + &sse_tool_call(
                    "throttle",
                    "good__query",
                    &serde_json::json!({"throttle":true}),
                )
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "s",
            "first",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls.len(), 3);
    assert_eq!(report.calls[0].state, "completed");
    assert_eq!(report.calls[1].state, "failed");
    assert_eq!(report.calls[2].state, "failed");
    assert!(
        report.calls[2]
            .output
            .contains("rate limited; wait before retrying")
    );
    assert_eq!(
        harness.db.list_tool_ops("s").unwrap().len(),
        3,
        "no automatic MCP retry"
    );
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM events WHERE kind='retry_scheduled'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "MCP RATE_LIMITED is a failed tool result, not generation retry"
    );
    assert!(
        report.calls[1]
            .output
            .contains("invalid arguments; check the tool schema")
    );
    assert_eq!(
        report.warnings,
        [format!(
            "mcp {} tools-list: transport (retryable=true)",
            super::mcp_lifecycle::diagnostic_name("broken")
        )]
    );
    let captured = requests.lock().unwrap().clone();
    assert_eq!(captured.len(), 2);
    for request in &captured {
        let wire = request.to_string();
        assert_eq!(wire.matches("GUIDANCE_GOOD").count(), 1);
        for absent in [
            "GUIDANCE_DENIED",
            "GUIDANCE_ASK",
            "GUIDANCE_EMPTY",
            "GUIDANCE_BROKEN",
            "GUIDANCE_DISABLED",
            "GUIDANCE_UNLISTED",
            "PROVIDER-CANARY",
            "HEADER-CANARY",
            "UNKNOWN-CANARY",
            "CONFIG-CANARY",
        ] {
            assert!(!wire.contains(absent), "leaked {absent}");
        }
    }
    let outputs: Vec<&serde_json::Value> = captured[1]["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .collect();
    assert_eq!(outputs.len(), 3);
    assert_eq!(outputs[2]["call_id"], "throttle");
    assert!(
        outputs[2]["output"]
            .as_str()
            .unwrap()
            .contains("rate limited; wait before retrying")
    );
    let data = outputs[0]["output"].as_str().unwrap();
    assert!(data.contains("Useful explanation"));
    assert!(data.contains("Embedded content"));
    assert!(data.contains("\"answer\":42"));
    assert!(data.contains("[redacted]"));
    let original = harness.db.read_history_full("s").unwrap();
    let stored = format!(
        "{original:?} {:?}",
        harness.db.turn_result(&report.turn_id).unwrap()
    );
    assert!(
        !stored.contains("GUIDANCE_"),
        "instructions must not enter durable history"
    );
    assert!(!stored.contains("CANARY"));

    // Resource-specific authority cannot inherit the scalar compatibility allow
    // when projecting whole-server guidance into a turn.
    generation.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
        &serde_json::json!({"permission":{"good__query":{"*":"deny","special":"allow"}}}),
    )
    .unwrap();
    runtime.reload(generation).await.unwrap();
    let report = runtime
        .run_turn(params(
            "s",
            "second",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert!(
        !requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .to_string()
            .contains("GUIDANCE_")
    );
    let after = harness.db.read_history_full("s").unwrap();
    assert_eq!(
        &after[..original.len()],
        original.as_slice(),
        "projection must not rewrite history"
    );
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn aud23_tool_list_changed_relists_only_the_dirty_server() {
    let (harness, mut generation) = make_harness(allow_all());
    let changed_script = harness._project.path().join("list_changed.py");
    let changed_log = harness._project.path().join("list_changed.log");
    let notification_gate = harness._project.path().join("first-request-completed");
    // The dirty server announces a change after every list, so a notification
    // arriving during a relist must stay pending for the next turn.
    std::fs::write(
        &changed_script,
        r#"import json, os, sys, time
count = 0
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        with open(sys.argv[1], 'a') as f: f.write('initialize\n')
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{'listChanged':True}},'serverInfo':{'name':'changed','version':'1'}}
    elif method == 'tools/list':
        count += 1
        with open(sys.argv[1], 'a') as f: f.write('list\n')
        result={'tools':[{'name':('old' if count % 2 == 1 else 'new'),'description':'changed','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
        if method == 'tools/list':
            if count == 1:
                while not os.path.exists(sys.argv[2]): time.sleep(0.01)
            print(json.dumps({'jsonrpc':'2.0','method':'notifications/tools/list_changed'}), flush=True)
"#,
    )
    .unwrap();
    let stable_script = harness._project.path().join("stable.py");
    let stable_log = harness._project.path().join("stable.log");
    std::fs::write(
        &stable_script,
        r#"import json, os, sys
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        with open(sys.argv[1], 'a') as f: f.write('initialize\n')
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'stable','version':'1'}}
    elif method == 'tools/list':
        with open(sys.argv[1], 'a') as f: f.write('list\n')
        result={'tools':[{'name':'ping','description':'stable','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    for (id, script, log) in [
        ("changed", &changed_script, &changed_log),
        ("stable", &stable_script, &stable_log),
    ] {
        generation.mcp.insert(
            id.into(),
            McpEntry {
                kind: "local".into(),
                url: None,
                enabled: true,
                oauth: false,
                headers: BTreeMap::new(),
                command: vec![
                    "/usr/bin/python3".into(),
                    script.to_string_lossy().into_owned(),
                    log.to_string_lossy().into_owned(),
                    notification_gate.to_string_lossy().into_owned(),
                ],
                timeout: Some(2_000),
                codemode: None,
                ..Default::default()
            },
        );
    }
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let (base, _, requests) =
        Fake::start_recording(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    for prompt in ["first", "second", "third"] {
        assert_eq!(
            runtime
                .run_turn(params(
                    "s",
                    prompt,
                    &harness,
                    provider_of(&base),
                    &NO_CANCEL,
                ))
                .await
                .unwrap()
                .status,
            TurnStatus::Completed
        );
        if prompt == "first" {
            std::fs::write(&notification_gate, "notify").unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    {
        let requests = requests.lock().unwrap();
        let names = |request: &serde_json::Value| {
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect::<Vec<_>>()
        };
        assert!(names(&requests[0]).contains(&"changed__old".to_string()));
        assert!(names(&requests[1]).contains(&"changed__new".to_string()));
        assert!(names(&requests[2]).contains(&"changed__old".to_string()));
        for request in requests.iter() {
            assert!(names(request).contains(&"stable__ping".to_string()));
        }
    }
    let changed = std::fs::read_to_string(&changed_log).unwrap();
    assert_eq!(
        changed.lines().filter(|line| *line == "initialize").count(),
        1
    );
    assert_eq!(
        changed.lines().filter(|line| *line == "list").count(),
        3,
        "notification during relist was lost: {changed}"
    );
    let stable = std::fs::read_to_string(&stable_log).unwrap();
    assert_eq!(
        stable.lines().filter(|line| *line == "initialize").count(),
        1
    );
    assert_eq!(
        stable.lines().filter(|line| *line == "list").count(),
        1,
        "dirty server forced an unrelated relist: {stable}"
    );
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn aud23_aborted_turn_releases_lease_and_shutdown_reaps_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("aborted_turn.py");
    let lifecycle = harness._project.path().join("aborted_turn.log");
    std::fs::write(
        &script,
        r#"import json, os, sys
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'aborted','version':'1'}}
    elif method == 'tools/list':
        result={'tools':[{'name':'ping','description':'ping','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "counted".into(),
        McpEntry {
            kind: "local".into(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".into(),
                script.to_string_lossy().into_owned(),
                lifecycle.to_string_lossy().into_owned(),
            ],
            timeout: Some(2_000),
            codemode: None,
            ..Default::default()
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (slow, _) = Fake::start(
        vec![sse_delta("slow") + &sse_completed()],
        Duration::from_secs(3),
    );
    {
        let pending = runtime.run_turn(params(
            "s",
            "aborted",
            &harness,
            provider_of(&slow),
            &NO_CANCEL,
        ));
        tokio::pin!(pending);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let spawned = std::fs::read_to_string(&lifecycle)
                .map(|log| log.contains("spawn "))
                .unwrap_or(false);
            if spawned {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "child never spawned");
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                _ = &mut pending => panic!("turn finished before the abort"),
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        // `pending` is dropped here while the provider stream is still open.
    }
    // The single-flight lease must be released by the dropped future.
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    assert_eq!(
        runtime
            .run_turn(params(
                "s",
                "after the abort",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .expect("lease released after abort")
            .status,
        TurnStatus::Completed
    );
    runtime.shutdown_mcp().await.unwrap();
    let pid = std::fs::read_to_string(&lifecycle)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("spawn "))
        .and_then(|pid| pid.parse::<libc::pid_t>().ok())
        .expect("recorded child pid");
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // SAFETY: signal 0 only probes the fixture child pid.
    while unsafe { libc::kill(pid, 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // SAFETY: final fixture liveness probe only.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_ne!(alive, 0, "shutdown left the generation child alive");
}

#[tokio::test]
async fn aud23_server_cap_blocks_spawn_before_first_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let marker = harness._project.path().join("cap-spawn.log");
    for index in 0..(oc_adapters::runtime::MAX_MCP_SERVERS + 1) {
        generation.mcp.insert(
            format!("server-{index}"),
            McpEntry {
                kind: "local".into(),
                url: None,
                enabled: true,
                oauth: false,
                headers: BTreeMap::new(),
                command: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    format!("printf 'spawned\\n' >> {}", marker.display()),
                ],
                timeout: Some(2_000),
                codemode: None,
                ..Default::default()
            },
        );
    }
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    let error = runtime
        .run_turn(params(
            "s",
            "must not attach",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect_err("server cap must refuse the generation");
    let text = error.to_string();
    assert!(
        text.contains("too many enabled MCP servers"),
        "actionable cap diagnostic: {text}"
    );
    assert!(!marker.exists(), "server cap spawned a child anyway");
    assert_eq!(harness.db.history_len("s").unwrap(), 0, "turn was accepted");
}

#[tokio::test]
async fn aud23_partial_attach_failure_degrades_and_still_reaps_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("partial_attach.py");
    let pid_file = harness._project.path().join("partial.pid");
    std::fs::write(
        &script,
        r#"import json, os, sys
with open(sys.argv[1], 'w') as f: f.write(str(os.getpid()))
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'partial','version':'1'}}
    elif method == 'tools/list': result={'tools':[]}
    if result is not None: print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "a-good".into(),
        McpEntry {
            kind: "local".into(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".into(),
                script.to_string_lossy().into_owned(),
                pid_file.to_string_lossy().into_owned(),
            ],
            timeout: Some(2_000),
            codemode: None,
            ..Default::default()
        },
    );
    generation.mcp.insert(
        "b-bad".into(),
        McpEntry {
            kind: "remote".into(),
            url: Some("http://127.0.0.1:9/v1/mcp".into()),
            enabled: true,
            oauth: false,
            headers: BTreeMap::from([(
                "Authorization".into(),
                "Bearer fixture-not-a-secret".into(),
            )]),
            command: Vec::new(),
            timeout: Some(200),
            codemode: None,
            ..Default::default()
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    wait_initial_mcp(&runtime).await;
    let report = runtime
        .run_turn(params(
            "s",
            "degraded peer",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("a degraded peer must not abort the turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        report.warnings,
        vec![format!(
            "mcp {} DNS: private_host (retryable=false)",
            super::mcp_lifecycle::diagnostic_name("b-bad")
        )]
    );
    let pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .parse::<libc::pid_t>()
        .unwrap();
    // SAFETY: signal 0 only probes the fixture child pid.
    let probe = unsafe { libc::kill(pid, 0) };
    assert_eq!(probe, 0, "healthy peer lost its child");
    // AUD23 still holds: generation shutdown reaps the connected child.
    runtime.shutdown_mcp().await.unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // SAFETY: signal 0 only probes the fixture child pid.
    while unsafe { libc::kill(pid, 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // SAFETY: final fixture liveness probe only.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_ne!(alive, 0, "partial attach leaked child");
    assert_eq!(harness.db.history_len("s").unwrap(), 2);
}

#[tokio::test]
async fn mcp_attach_failure_degrades_the_server() {
    let (harness, mut generation) = make_harness(allow_all());
    generation.mcp.insert(
        "codex".to_string(),
        McpEntry {
            kind: "remote".to_string(),
            url: Some("http://127.0.0.1:9/v1/mcp".to_string()),
            enabled: true,
            oauth: false,
            headers: [("authorization".to_string(), "Bearer k".to_string())]
                .into_iter()
                .collect(),
            command: Vec::new(),
            timeout: None,
            codemode: None,
            ..Default::default()
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    wait_initial_mcp(&runtime).await;
    let (base, _) = Fake::start(vec![sse_delta("hi") + &sse_completed()], Duration::ZERO);
    let report = runtime
        .run_turn(params("s", "hi", &harness, provider_of(&base), &NO_CANCEL))
        .await
        .expect("attach failure must degrade, not abort");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        report.warnings,
        vec![format!(
            "mcp {} DNS: private_host (retryable=false)",
            super::mcp_lifecycle::diagnostic_name("codex")
        )]
    );
    assert!(
        report.calls.is_empty(),
        "degraded server published no tools"
    );
    assert_eq!(
        harness.db.history_len("s").expect("len"),
        2,
        "turn committed"
    );
}

#[tokio::test]
async fn canonical_tool_cards_keep_output_order_and_durable_read_pairing_after_restart() {
    for before_text in [false, true] {
        let (mut harness, generation) = make_harness(allow_all());
        std::fs::write(
            harness._project.path().join("fixture.txt"),
            "fixture contents\n",
        )
        .unwrap();
        let runtime = runtime_of(&harness, generation.clone(), Vec::new());
        runtime.create_session("ordered-tools").unwrap();
        let call = serde_json::json!({"type":"function_call", "id":"fc_ordered",
            "call_id":"ordered", "name":"read", "arguments":"{\"path\":\"fixture.txt\"}",
            "status":"completed"});
        let first = serde_json::json!({"type":"message", "id":"msg_first", "role":"assistant",
            "status":"completed", "content":[{"type":"output_text", "text":"before"}]});
        let second = serde_json::json!({"type":"message", "id":"msg_second", "role":"assistant",
            "status":"completed", "content":[{"type":"output_text", "text":"after"}]});
        let reasoning = serde_json::json!({"type":"reasoning", "id":"rs_ordered",
            "encrypted_content":"private-ordered", "summary":[], "status":"completed"});
        let (stream, output) = if before_text {
            (
                sse_reasoning("Checking")
                    + &sse_reasoning_done("rs_ordered", "private-ordered")
                    + &sse_message_done(1, &first)
                    + &sse_tool_call(
                        "ordered",
                        "read",
                        &serde_json::json!({"path":"fixture.txt"}),
                    )
                    + &sse_message_done(3, &second),
                vec![reasoning, first, call, second],
            )
        } else {
            (
                sse_tool_call(
                    "ordered",
                    "read",
                    &serde_json::json!({"path":"fixture.txt"}),
                ) + &sse_message_done(1, &first),
                vec![call, first],
            )
        };
        let (base, hits, requests) = Fake::start_recording(
            vec![
                stream + &sse_completed_output(output.clone()),
                sse_delta("done") + &sse_completed(),
                sse_delta("after restart") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let report = runtime
            .run_turn(params(
                "ordered-tools",
                "inspect",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        assert_eq!(report.rounds, 2);
        assert_eq!(report.calls.len(), 1);
        assert_eq!(report.calls[0].state, "completed");
        assert_eq!(
            report.text,
            if before_text {
                "beforeafterdone"
            } else {
                "beforedone"
            }
        );
        let ops = harness.db.list_tool_ops("ordered-tools").unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].state, "completed");
        let (_, raw) = harness.db.turn_result(&report.turn_id).unwrap();
        let stored: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
        let parts = stored["display_parts"].as_array().unwrap();
        let markers = parts
            .iter()
            .map(|part| {
                if part.get("reasoning").is_some() {
                    "reasoning"
                } else if part.get("tool").is_some() {
                    "tool"
                } else {
                    "message"
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            markers,
            if before_text {
                vec!["reasoning", "message", "tool", "message", "message"]
            } else {
                vec!["tool", "message", "message"]
            }
        );
        let card = parts
            .iter()
            .find(|part| part.get("tool").is_some())
            .unwrap();
        assert_eq!(card["tool"], ops[0].op);
        let messages = parts
            .iter()
            .filter_map(|part| part["message"].as_u64())
            .map(|index| {
                stored["input"][index as usize]["content"][0]["text"]
                    .as_str()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            if before_text {
                vec!["before", "after", "done"]
            } else {
                vec!["before", "done"]
            }
        );
        assert!(
            !stored["display_parts"]
                .to_string()
                .contains("private-ordered")
        );
        assert!(!stored["display_parts"].to_string().contains("pending_text"));
        assert_eq!(
            harness.db.read_history("ordered-tools").unwrap(),
            [
                ("user".into(), "inspect".into()),
                ("assistant".into(), report.text.clone())
            ]
        );
        for (offset, item) in output.iter().enumerate() {
            assert_eq!(stored["input"][offset + 1], *item);
        }
        let paired = &stored["input"][output.len() + 1];
        assert_eq!(paired["type"], "function_call_output");
        assert_eq!(paired["call_id"], "ordered");
        assert!(
            paired["output"]
                .as_str()
                .unwrap()
                .contains("fixture contents")
        );
        assert_eq!(*hits.lock().unwrap(), 2);
        {
            let requests_before = requests.lock().unwrap();
            let second_input = requests_before[1]["input"].as_array().unwrap();
            assert_eq!(
                &second_input[second_input.len() - output.len() - 1..second_input.len() - 1],
                output
            );
            assert_eq!(
                function_output(&requests_before[1], "ordered"),
                paired["output"].as_str()
            );
        }

        drop(runtime);
        drop(harness.db);
        harness.db = Db::open(harness._data.path()).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &harness.db.turn_result(&report.turn_id).unwrap().1.unwrap()
            )
            .unwrap(),
            stored
        );
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.open_session("ordered-tools").unwrap();
        let resumed = runtime
            .run_turn(params(
                "ordered-tools",
                "continue",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(resumed.status, TurnStatus::Completed);
        assert!(resumed.calls.is_empty());
        assert_eq!(harness.db.list_tool_ops("ordered-tools").unwrap().len(), 1);
        assert_eq!(*hits.lock().unwrap(), 3);
        let requests = requests.lock().unwrap();
        let replay = requests[2]["input"].as_array().unwrap();
        for item in &output {
            assert_eq!(
                replay.iter().filter(|candidate| *candidate == item).count(),
                1
            );
        }
        assert_eq!(
            function_output(&requests[2], "ordered"),
            paired["output"].as_str()
        );
        assert_eq!(
            harness.db.read_history("ordered-tools").unwrap(),
            [
                ("user".into(), "inspect".into()),
                ("assistant".into(), report.text),
                ("user".into(), "continue".into()),
                ("assistant".into(), "after restart".into()),
            ]
        );
    }
}

#[tokio::test]
async fn unidentifiable_calls_append_only_at_intent_and_duplicate_call_ids_fail_closed() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("unidentified-tool").unwrap();
    let call = serde_json::json!({"type":"function_call", "call_id":"read-one",
        "name":"read", "arguments":"{\"path\":\"fixture.txt\"}"});
    let message = serde_json::json!({"type":"message", "id":"msg_one", "role":"assistant",
        "content":[{"type":"output_text", "text":"hello"}]});
    std::fs::write(harness._project.path().join("fixture.txt"), "present").unwrap();
    let (base, hits) = Fake::start(
        vec![
            sse_completed_output(vec![call.clone(), message.clone()]),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    let request = params(
        "unidentified-tool",
        "read",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    let report = runtime.run_turn(request).await.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.rounds, 2);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].state, "completed");
    assert_eq!(*hits.lock().unwrap(), 2);
    let (_, raw) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
    let parts = stored["display_parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["message"], 2);
    assert_eq!(
        parts[1]["tool"],
        harness.db.list_tool_ops("unidentified-tool").unwrap()[0].op
    );
    assert_eq!(stored["input"][1], call);
    assert_eq!(stored["input"][2], message);
    assert_eq!(stored["input"][3]["call_id"], "read-one");

    runtime.create_session("duplicate-tool").unwrap();
    let duplicate = serde_json::json!({"type":"function_call", "id":"fc_second",
        "call_id":"read-one", "name":"read", "arguments":"{\"path\":\"fixture.txt\"}"});
    let (base, hits) = Fake::start(
        vec![sse_completed_output(vec![
            stored["input"][1].clone(),
            duplicate,
        ])],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "duplicate-tool",
            "read",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Failed);
    assert_eq!(
        report.rounds, 0,
        "duplicate identity rejected before successful Generation"
    );
    assert_eq!(*hits.lock().unwrap(), 1);
    assert!(
        harness
            .db
            .list_tool_ops("duplicate-tool")
            .unwrap()
            .is_empty()
    );
    let (_, raw) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
    assert!(stored["display_parts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn argument_stream_split_json_multiround_exact_links_and_duplicate_refusal() {
    use oc_core::tool_stream::ToolStreamEvent;
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("stream-identities").unwrap();
    std::fs::write(harness._project.path().join("fixture.txt"), "present").unwrap();
    let added = serde_json::json!({"type":"response.output_item.added","item":{
        "type":"function_call","id":"fc_reused","call_id":"reused","name":"read","arguments":"","status":"in_progress"
    }});
    let mut body = format!("data: {added}\n\n");
    for delta in ["{\"pa", "th\":\"fixture", ".txt\"}"] {
        let event = serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"fc_reused","delta":delta});
        body.push_str(&format!("data: {event}\n\n"));
    }
    body.push_str(&sse_completed_output(vec![serde_json::json!({"type":"function_call","id":"fc_reused","call_id":"reused","name":"read","arguments":"{\"path\":\"fixture.txt\"}"})]));
    let (base, _) = Fake::start(
        vec![
            body.clone(),
            body.clone(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut events = Vec::new();
    let report = runtime
        .run_turn_with_tool_events(
            params(
                "stream-identities",
                "read twice",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| events.push(event.clone()),
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    let links = events
        .iter()
        .filter_map(|event| match event {
            ToolCallEvent::ArgumentStream(ToolStreamEvent::Linked { identity, op }) => {
                Some((identity, op))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(links.len(), 2);
    assert_eq!((links[0].0.round, links[1].0.round), (1, 2));
    assert_eq!(links[0].0.item_id, "fc_reused");
    assert_eq!(links[0].0.call_id, links[1].0.call_id);
    assert_ne!(links[0].1, links[1].1);
    let previews = events
        .iter()
        .filter_map(|event| match event {
            ToolCallEvent::ArgumentStream(ToolStreamEvent::Pending { preview, .. })
                if !preview.is_empty() =>
            {
                Some(preview)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        previews,
        [
            &"{\"path\":\"fixture.txt\"}".to_string(),
            &"{\"path\":\"fixture.txt\"}".to_string()
        ]
    );
    assert_eq!(
        harness.db.list_tool_ops("stream-identities").unwrap().len(),
        2
    );
    runtime.create_session("stream-duplicates").unwrap();
    let duplicate = sse_tool_call("reused", "read", &serde_json::json!({"path":"fixture.txt"}))
        .replace("fc_reused", "fc_other");
    let first = serde_json::json!({"type":"function_call","id":"fc_reused","call_id":"reused","name":"read","arguments":"{\"path\":\"fixture.txt\"}"});
    let (base, _) = Fake::start(
        vec![
            body.replace("response.completed", "fixture.ignored")
                + &sse_message_done_by_id(&first)
                + &duplicate
                + &sse_completed(),
        ],
        Duration::ZERO,
    );
    events.clear();
    let report = runtime
        .run_turn_with_tool_events(
            params(
                "stream-duplicates",
                "duplicate",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| events.push(event.clone()),
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Failed);
    assert!(!events.iter().any(|event| matches!(
        event,
        ToolCallEvent::Started { .. }
            | ToolCallEvent::ArgumentStream(ToolStreamEvent::Linked { .. })
    )));
    assert!(
        harness
            .db
            .list_tool_ops("stream-duplicates")
            .unwrap()
            .is_empty()
    );
}

/// TUI tool cards are built from real runtime state: one `apply_patch` call
/// produces `Started` then `Finished` events after the durable records, and
/// the recorded operation carries the patch text the transcript renders.
#[tokio::test]
async fn dto_tool_events_surface_started_and_finished_with_a_patch() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s-tools").unwrap();
    std::fs::write(
        harness._project.path().join("old.txt"),
        "prefix\nold\nsuffix",
    )
    .unwrap();
    let patch = "*** Begin Patch\n*** Add File: added.txt\n+hello\n*** Update File: old.txt\n@@\n-old\n+new\n*** End Patch";
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "call-patch",
                "apply_patch",
                &serde_json::json!({"patchText": patch}),
            ) + &sse_completed(),
            sse_delta("patched") + &sse_completed(),
        ],
        Duration::from_millis(5),
    );
    let mut events = Vec::new();
    let mut text = String::new();
    let report = runtime
        .run_turn_with_tool_events(
            params(
                "s-tools",
                "patch it",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, delta| text.push_str(delta),
            |_, _| {},
            |_, event| events.push(event.clone()),
        )
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(text, "patched");
    assert!(events.iter().any(|event| matches!(event, ToolCallEvent::ArgumentStream(oc_core::tool_stream::ToolStreamEvent::Pending { name, .. }) if name == "apply_patch")));
    let linked = events
        .iter()
        .find_map(|event| match event {
            ToolCallEvent::ArgumentStream(oc_core::tool_stream::ToolStreamEvent::Linked {
                identity,
                op,
            }) => Some((identity.clone(), op.clone())),
            _ => None,
        })
        .expect("exact durable link");
    assert_eq!(linked.0.call_id, "call-patch");
    assert_eq!(linked.0.round, 1);
    events.retain(|event| !matches!(event, ToolCallEvent::ArgumentStream(_)));
    assert_eq!(events.len(), 2, "one intent and one outcome: {events:?}");
    match &events[0] {
        ToolCallEvent::Started {
            dcp_topic,
            op,
            name,
            input,
        } => {
            assert!(dcp_topic.is_none(), "ordinary tools have no DCP topic");
            assert_eq!(op, &linked.1);
            assert_eq!(name, "apply_patch");
            assert!(!op.is_empty());
            assert!(
                input.contains("*** Add File: added.txt"),
                "the recorded input carries the patch: {input}"
            );
        }
        other => panic!("expected Started, got {other:?}"),
    }
    match &events[1] {
        ToolCallEvent::Finished {
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
            patch_effects,
            ..
        } => {
            let effects = patch_effects.as_ref().expect("confirmed effects");
            assert_eq!(effects.total_files, 2);
            assert_eq!(effects.files[0].additions, 1);
            assert_eq!(effects.files[1].deletions, 1);
            let lines = &effects.files[1].hunks[0].lines;
            assert_eq!(lines[0].text, "prefix");
            assert_eq!(lines[0].kind, oc_core::patch::PatchLineKind::Context);
            assert_eq!((lines[0].old_line, lines[0].new_line), (Some(1), Some(1)));
            let last = lines.last().unwrap();
            assert_eq!(last.text, "suffix");
            assert_eq!((last.old_line, last.new_line), (Some(3), Some(3)));
            assert_eq!(last.ending, oc_core::patch::LineEnding::None);
            assert_eq!(name, "apply_patch");
            assert_eq!(state, "completed");
            assert!(!op.is_empty());
            assert!(output.contains("added.txt"), "{output}");
            assert!(*output_bytes > 0);
            assert!(!output_truncated, "small outputs are not truncated");
        }
        other => panic!("expected Finished, got {other:?}"),
    }
    // The patch really ran and the operation is durably recorded.
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("added.txt")).unwrap(),
        "hello\n"
    );
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("old.txt")).unwrap(),
        "prefix\nnew\nsuffix"
    );
    let ops = harness.db.list_tool_ops("s-tools").unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name, "apply_patch");
    assert_eq!(ops[0].state, "completed");
    let ToolCallEvent::Finished { patch_effects, .. } = &events[1] else {
        unreachable!()
    };
    assert_eq!(&ops[0].patch_effects, patch_effects);
    // Remove the entire mutation workspace: replay must use storage alone.
    std::fs::remove_dir_all(harness._project.path()).unwrap();
    let replay = harness.db.list_tool_ops_page("s-tools", 10, None).unwrap();
    assert_eq!(&replay[0].patch_effects, patch_effects);
    assert!(
        ops[0]
            .input
            .as_deref()
            .is_some_and(|input| input.contains("*** Add File: added.txt")),
        "the durable intent keeps the patch text the card renders"
    );
    drop(runtime);
    let Harness { db, _data, .. } = harness;
    drop(db);
    let reopened = Db::open(_data.path()).unwrap();
    let replay = reopened.list_tool_ops_page("s-tools", 10, None).unwrap();
    assert_eq!(&replay[0].patch_effects, patch_effects);
}

#[tokio::test]
async fn vis35_serialized_cap_finish_checkpoint_attach_restart_match() {
    // This used to fit the text-only producer cap while serializing to 78 KiB;
    // the 66 KiB input also used to be admitted before reserving the effects.
    check_application_patch_replay(&"x".repeat(546), 120).await;
}
