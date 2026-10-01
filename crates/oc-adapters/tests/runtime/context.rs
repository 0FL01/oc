//! DCP, projection, context/model admission and scoped protection.

use super::*;

#[tokio::test]
async fn primary_selection_and_restore_narrow_dispatch_guidance_and_children() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mcp_script = project.path().join("permissions_mcp.py");
    std::fs::write(&mcp_script, r#"import json, sys
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'},'instructions':'PRIMARY_POLICY_GUIDANCE'}
    elif method == 'tools/list':
        result={'tools':[{'name':'query','inputSchema':{'type':'object'}}]}
    elif method == 'tools/call':
        with open(sys.argv[1], 'a') as f: f.write('called\n')
        result={'content':[{'type':'text','text':'MCP allowed'}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#).unwrap();
    let batch = |prefix: &str, child: bool| {
        let mut result = sse_tool_call(
            &format!("{prefix}-bash"),
            "bash",
            &serde_json::json!({"argv":["/bin/touch", format!("{prefix}-marker")]}),
        ) + &sse_tool_call(
            &format!("{prefix}-mcp"),
            "fixture__query",
            &serde_json::json!({}),
        );
        if child {
            result += &sse_tool_call(
                "spawn",
                "subagent",
                &serde_json::json!({
                    "agent":"helper", "description":"Check inherited policy", "prompt":"try tools"
                }),
            );
        }
        result + &sse_completed()
    };
    let done = || sse_delta("done") + &sse_completed();
    let (base, _, requests) = Fake::start_recording(
        vec![
            batch("startup", false),
            done(),
            batch("build", false),
            done(),
            batch("review", true),
            batch("child", false),
            done(),
            done(),
            batch("restored", false),
            done(),
        ],
        Duration::ZERO,
    );
    let mut config = serde_json::json!({
        "model":"fixture/main", "default_agent":"review",
        "provider":{"fixture":{
            "npm":"@ai-sdk/openai", "options":{"baseURL":base,"apiKey":"fixture-key"},
            "models":{"main":{"limit":{"context":65536,"output":4096}}}
        }},
        "permission":"allow",
        "agent":{
            "build":{"mode":"primary", "prompt":"BUILD_PRIMARY", "permission":"allow"},
            "review":{"mode":"primary", "prompt":"REVIEW_PRIMARY", "tools":{"bash":false,"fixture_query":false}},
            "helper":{"mode":"subagent", "prompt":"HELPER_CHILD", "permission":"allow"}
        },
        "mcp":{"fixture":{"type":"local", "command":["/usr/bin/python3",mcp_script,project.path().join("mcp-calls")], "timeout":2000}}
    });
    let config_path = project.path().join("opencode.json");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env.clone())
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.mcp_status().await.unwrap().servers[0].status
        != oc_core::queries::McpStatus::Connected
    {
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let session = SessionId::new("primary-policy").unwrap();
    app.create_session(session.clone()).await.unwrap();
    // This fixture scripts conversational requests by arrival index. Pin its
    // title so the independent automatic title lane cannot consume that queue.
    app.rename_session(session.clone(), "Policy fixture".into())
        .await
        .unwrap();
    let mut events = app.subscribe();
    for selected in [None, Some("build"), Some("review")] {
        if let Some(id) = selected {
            assert_eq!(
                app.select_agent(id.into())
                    .await
                    .unwrap()
                    .agent_id
                    .as_deref(),
                Some(id)
            );
        }
        app.submit(session.clone(), "try tools".into())
            .await
            .unwrap();
        loop {
            match tokio::time::timeout(Duration::from_secs(10), events.recv())
                .await
                .unwrap()
                .unwrap()
            {
                CoreEvent::TurnFinished { .. } => break,
                CoreEvent::TurnFailed { error, .. } => panic!("turn failed: {error}"),
                _ => {}
            }
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    // Restore a non-default selection, including when no spawnable catalog exists.
    config["default_agent"] = "build".into();
    config["agent"].as_object_mut().unwrap().remove("helper");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.mcp_status().await.unwrap().servers[0].status
        != oc_core::queries::McpStatus::Connected
    {
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        app.catalog().await.unwrap().agent_id.as_deref(),
        Some("review")
    );
    let mut events = app.subscribe();
    app.submit(session, "try restored tools".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { .. } => break,
            CoreEvent::TurnFailed { error, .. } => panic!("restored turn failed: {error}"),
            _ => {}
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    assert!(
        project.path().join("build-marker").exists(),
        "startup review constraints stuck to central authority"
    );
    for denied in ["startup", "review", "child", "restored"] {
        assert!(
            !project.path().join(format!("{denied}-marker")).exists(),
            "{denied} widened bash"
        );
    }
    assert_eq!(
        std::fs::read_to_string(project.path().join("mcp-calls")).unwrap(),
        "called\n"
    );
    let captured = requests.lock().unwrap();
    assert_eq!(captured.len(), 10);
    for (index, prefix) in [(1, "startup"), (6, "child"), (7, "review"), (9, "restored")] {
        assert_eq!(
            function_output(&captured[index], &format!("{prefix}-bash")),
            Some("error: denied bash")
        );
        assert_eq!(
            function_output(&captured[index], &format!("{prefix}-mcp")),
            Some("error: denied fixture__query")
        );
    }
    assert!(
        function_output(&captured[3], "build-bash")
            .unwrap()
            .contains("exit 0")
    );
    assert!(
        function_output(&captured[3], "build-mcp")
            .unwrap()
            .contains("MCP allowed")
    );
    assert!(
        function_output(&captured[7], "spawn")
            .unwrap()
            .contains("done")
    );
    for (index, request) in captured.iter().enumerate() {
        assert_eq!(
            request.to_string().contains("PRIMARY_POLICY_GUIDANCE"),
            matches!(index, 2 | 3),
            "guidance in request {index}"
        );
    }
    assert!(captured[5]["input"].to_string().contains("HELPER_CHILD"));
    assert!(captured[8]["input"].to_string().contains("REVIEW_PRIMARY"));
}

#[tokio::test]
async fn primary_workspace_policy_also_bounds_manual_compress() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("primary-compress").unwrap();
    let start = harness
        .db
        .append_message("primary-compress", "user", &"input ".repeat(100))
        .unwrap();
    let end = harness
        .db
        .append_message("primary-compress", "assistant", &"output ".repeat(100))
        .unwrap();
    harness
        .db
        .append_message("primary-compress", "user", "next task")
        .unwrap();
    let args = serde_json::json!({"topic":"finished", "content":[{"startId":start,"endId":end,"summary":"done"}]});
    let spec = ProtectedSpec {
        protect_user_messages: false,
        protect_tags: false,
        ..ProtectedSpec::default()
    };
    let rules = oc_adapters::permissions::PermissionRules::from_config(
        &serde_json::json!({"tools":{"compress":false}}),
    )
    .unwrap();
    runtime
        .publish_workspace(
            None,
            "",
            Vec::new(),
            BTreeMap::new(),
            None,
            Some("review".into()),
            None,
            BTreeMap::new(),
            rules,
        )
        .unwrap();
    assert_eq!(
        runtime
            .run_compress("primary-compress", &args, &spec)
            .unwrap_err(),
        oc_adapters::runtime::RuntimeError::PermissionDenied {
            tool: "compress".into()
        }
    );
    assert!(
        harness
            .db
            .list_tool_ops("primary-compress")
            .unwrap()
            .is_empty()
    );
    assert!(
        harness
            .db
            .load_compression_blocks("primary-compress")
            .unwrap()
            .is_empty()
    );
    runtime
        .publish_workspace(
            None,
            "",
            Vec::new(),
            BTreeMap::new(),
            None,
            Some("build".into()),
            None,
            BTreeMap::new(),
            Default::default(),
        )
        .unwrap();
    assert!(
        runtime
            .run_compress("primary-compress", &args, &spec)
            .unwrap()
            .shrank
    );
}

#[tokio::test]
async fn t47_unknown_limits_use_configured_caps_without_variant_overlay() {
    for limit in [
        serde_json::Value::Null,
        serde_json::json!({"context": 16_384}),
        serde_json::json!({"output": 128}),
        serde_json::json!({"output": 100_000}),
        serde_json::json!({"context": 0, "output": 0}),
        serde_json::json!({"context": 16_384, "output": 0}),
        serde_json::json!({"context": 0, "output": 128}),
    ] {
        let (mut harness, _) = make_harness(allow_all());
        let entry = serde_json::json!({"limit": limit, "variants": {"low": {"reasoningEffort": "low"}, "custom": {"reasoningEffort": "deep"}}});
        harness.catalog.models.insert("m".into(), entry.clone());
        let mut generation = oc_adapters::config::assemble(&[oc_adapters::config::Source {
            path: "test.json".into(), trusted: true,
            text: serde_json::json!({"provider":{"test":{"options":{"apiKey":"test-key", "nativeFallbackLimits":{"context":16_384,"output":256}}}}}).to_string(),
        }], &BTreeMap::new(), None).unwrap();
        generation.permissions = allow_all();
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").unwrap();
        let (base, hits, requests) =
            Fake::start_recording(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
        let mut turn = params("s", "hello", &harness, provider_of(&base), &NO_CANCEL);
        turn.max_output = 0; // Application's absent-output path.
        let report = runtime.run_turn(turn).await.unwrap();
        assert_eq!(report.status, TurnStatus::Completed, "{report:?}");
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("unknown"));
        assert!(report.warnings[0].contains("native fallback caps (context=16384, output=256)"));
        assert_eq!(harness.catalog.models["m"], entry);
        let expected_output = limit
            .get("output")
            .and_then(serde_json::Value::as_u64)
            .filter(|n| *n > 0)
            .unwrap_or(256)
            .min(256);
        {
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0]["max_output_tokens"], expected_output);
            assert!(requests[0].get("reasoning").is_none(), "{:?}", requests[0]);
        }
        let huge = "x".repeat(80_000);
        let result = runtime
            .run_turn(params("s", &huge, &harness, provider_of(&base), &NO_CANCEL))
            .await;
        assert!(result.unwrap_err().to_string().contains("exceeds context"));
        assert_eq!(
            *hits.lock().unwrap(),
            1,
            "over-budget request reached provider"
        );
        runtime.shutdown_mcp().await.unwrap();
    }
}

#[tokio::test]
async fn v04_model_change_projects_public_history_without_foreign_tool_state() {
    let (mut harness, generation) = make_harness(allow_all());
    harness
        .catalog
        .models
        .insert("other".into(), harness.catalog.models["m"].clone());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("model-change").unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call(
                "old-model-call",
                "read",
                &serde_json::json!({"filePath":"missing-fixture"}),
            ) + &sse_completed(),
            sse_delta("public original answer") + &sse_completed(),
            sse_delta("public second answer") + &sse_completed(),
            sse_delta("returned original") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    for (index, (model, prompt)) in [
        ("m", "expanded review instructions with original config"),
        ("other", "second public input"),
        ("m", "third public input"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut turn = params(
            "model-change",
            prompt,
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        );
        turn.model_id = model.into();
        if index == 0 {
            turn.invocation = Some("/review".into());
        }
        assert_eq!(
            runtime.run_turn(turn).await.unwrap().status,
            TurnStatus::Completed
        );
    }
    {
        let captured = requests.lock().unwrap();
        assert_eq!(captured.len(), 4);
        assert!(captured[1]["input"].to_string().contains("old-model-call"));
        let changed = captured[2]["input"].to_string();
        assert!(changed.contains("expanded review instructions with original config"));
        assert!(changed.contains("public original answer"));
        assert!(!changed.contains("/review"));
        assert!(!changed.contains("old-model-call") && !changed.contains("function_call"));
        assert_eq!(captured[2]["model"], "other");
        assert!(
            captured[3]["input"].to_string().contains("old-model-call"),
            "original wire lane retained durably"
        );
        assert!(
            captured[3]["input"]
                .to_string()
                .contains("public second answer")
        );
    }
    // Legacy/incomplete log without a usable stored prompt: fall back to the
    // immutable public invocation rather than expanding current command config.
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    assert_eq!(
        sql.execute(
            "UPDATE turns SET prompt = '' WHERE session_id = ?1 AND prompt = ?2",
            rusqlite::params![
                "model-change",
                "expanded review instructions with original config"
            ]
        )
        .unwrap(),
        1
    );
    let mut fallback = params(
        "model-change",
        "fourth public input",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    fallback.model_id = "other".into();
    assert_eq!(
        runtime.run_turn(fallback).await.unwrap().status,
        TurnStatus::Completed
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    let fallback_input = requests[4]["input"].to_string();
    assert!(fallback_input.contains("/review"));
    assert!(!fallback_input.contains("expanded review instructions"));
    assert!(!fallback_input.contains("old-model-call"));
    let public = harness.db.read_history("model-change").unwrap();
    assert_eq!(public.len(), 8);
    assert_eq!(public[0], ("user".into(), "/review".into()));
    assert!(
        !public
            .iter()
            .any(|(_, text)| text.contains("expanded review instructions"))
    );
}

#[tokio::test]
async fn t47_admission_counts_tool_schemas_and_rechecks_tool_results() {
    for (context, expected_calls) in [(1_200, 0), (8_192, 1)] {
        let (mut harness, mut generation) = make_harness(allow_all());
        harness
            .catalog
            .models
            .insert("m".into(), serde_json::json!({}));
        generation.providers.insert(
            "test".into(),
            serde_json::from_value(serde_json::json!({
                "options":{"nativeFallbackLimits":{"context":context,"output":64}}
            }))
            .unwrap(),
        );
        let large = harness._project.path().join("large.txt");
        std::fs::write(
            &large,
            "large fact with retained original detail\n".repeat(4_000),
        )
        .unwrap();
        let runtime = runtime_with_dcp(
            &harness,
            generation,
            Vec::new(),
            DcpConfig {
                enabled: false,
                ..DcpConfig::default()
            },
        );
        runtime.create_session("s").unwrap();
        let script = sse_tool_call(
            "read-large",
            "read",
            &serde_json::json!({"path":"large.txt", "limit":2_000}),
        ) + &sse_completed();
        let (base, hits, _) = Fake::start_recording(vec![script], Duration::ZERO);
        let result = runtime
            .run_turn(params(
                "s",
                "read",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await;
        if expected_calls == 0 {
            // The irreducible request now includes tool schemas before durable
            // admission or compaction spend, rather than failing a started turn.
            assert!(
                matches!(&result, Err(RuntimeError::InvalidArgs(message)) if message.contains("exceeds context"))
            );
            assert_eq!(*hits.lock().unwrap(), 0);
            assert!(harness.db.read_history("s").unwrap().is_empty());
            assert!(harness.db.compaction_history("s").unwrap().is_empty());
            runtime.shutdown_mcp().await.unwrap();
            continue;
        }
        let report = result.unwrap();
        assert_eq!(report.status, TurnStatus::Failed, "{report:?}");
        assert!(
            report
                .diagnostic
                .as_deref()
                .unwrap()
                .contains("exceeds context")
        );
        assert_eq!(report.rounds, expected_calls);
        assert_eq!(*hits.lock().unwrap(), expected_calls as usize);
        assert_eq!(report.warnings.len(), 1);
        if expected_calls > 0 {
            let operations = harness.db.list_tool_ops("s").unwrap();
            assert_eq!(operations.len(), 1);
            assert_eq!(operations[0].state, "completed");
            assert!(
                operations[0]
                    .output
                    .as_deref()
                    .unwrap()
                    .contains("large fact")
            );
            assert!(
                operations[0].output_bytes > 32_768,
                "oversized result must stay durable"
            );
        }
        runtime.shutdown_mcp().await.unwrap();
    }
}

#[tokio::test]
async fn compress_blocks_compensate_and_stabilize() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    for (i, role) in [
        "user",
        "assistant",
        "user",
        "assistant",
        "user",
        "assistant",
    ]
    .iter()
    .enumerate()
    {
        harness
            .db
            .append_message("s", role, &format!("message {i} body"))
            .expect("msg");
    }
    let ids: Vec<(String, String, String)> = harness.db.read_history_full("s").expect("ids");
    // Binary startup applies the DCP schema; the harness mirrors that wiring.
    oc_adapters::dcp::apply_dcp_schema(&harness.db).expect("dcp schema");
    let spec = ProtectedSpec {
        protect_user_messages: false,
        protect_tags: false,
        file_globs: Vec::new(),
        ..ProtectedSpec::default()
    };
    let args = serde_json::json!({
        "topic": "t",
        "content": [{"startId": ids[0].0, "endId": ids[1].0, "summary": "first"}],
    });
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_compress_intent BEFORE INSERT ON tool_operations BEGIN SELECT RAISE(ABORT, 'injected compress intent'); END;").unwrap();
    assert_eq!(
        runtime.run_compress("s", &args, &spec).unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert!(harness.db.load_compression_blocks("s").unwrap().is_empty());
    sql.execute_batch("DROP TRIGGER fail_compress_intent")
        .unwrap();
    let report = runtime.run_compress("s", &args, &spec).expect("compress");
    assert_eq!(report.blocks, ["b0001".to_string()]);
    assert!(report.shrank);

    let args2 = serde_json::json!({
        "topic": "t",
        "content": [{"startId": ids[2].0, "endId": ids[3].0, "summary": "second"}],
    });
    let report2 = runtime.run_compress("s", &args2, &spec).expect("compress2");
    assert_eq!(
        report2.blocks,
        ["b0002".to_string()],
        "stable ids across calls"
    );

    // Invalid args store nothing.
    let before = harness
        .db
        .load_compression_blocks("s")
        .expect("blocks")
        .len();
    let bad = serde_json::json!({"topic": "t", "content": []});
    assert!(runtime.run_compress("s", &bad, &spec).is_err());
    let after = harness
        .db
        .load_compression_blocks("s")
        .expect("blocks")
        .len();
    assert_eq!(before, after);

    // A valid but ineffective manual operation is durably no_gain, not a
    // completed operation lacking a snapshot or a generic storage failure.
    let accounting = harness.db.dcp_accounting("s").unwrap();
    let no_gain = serde_json::json!({"topic":"honest no gain","content":[{"startId":ids[4].0,"endId":ids[4].0,"summary":"ineffective summary ".repeat(100)}]});
    assert!(runtime.run_compress("s", &no_gain, &spec).is_err());
    let operation = harness
        .db
        .list_tool_ops("s")
        .unwrap()
        .into_iter()
        .last()
        .unwrap();
    assert_eq!(operation.state, "no_gain");
    assert_eq!(operation.dcp_topic.as_deref(), Some("honest no gain"));
    assert!(operation.dcp.is_none());
    let output: serde_json::Value =
        serde_json::from_str(operation.output.as_deref().unwrap()).unwrap();
    assert_eq!(output["status"], "no_gain");
    assert_eq!(harness.db.dcp_accounting("s").unwrap(), accounting);

    // Compress obeys the same permission path (default-deny without entry).
    let (harness2, generation2) = make_harness(BTreeMap::new());
    let runtime2 = runtime_of(&harness2, generation2, Vec::new());
    runtime2.create_session("s2").expect("create");
    assert!(runtime2.run_compress("s2", &args, &spec).is_err());
}

#[tokio::test]
async fn aud20_nudge_cadence_and_model_compress_are_session_scoped() {
    let (harness, generation) = make_harness(allow_all());
    let dcp = DcpConfig {
        min_context: 1,
        max_context: 1,
        nudge_frequency: 2,
        iteration_threshold: 100,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.create_session("a").unwrap();
    runtime.create_session("b").unwrap();
    std::fs::write(harness._project.path().join("note.txt"), "note").unwrap();

    let start = harness
        .db
        .append_message("a", "user", &format!("closed start {}", "x".repeat(8_192)))
        .unwrap();
    let end = harness
        .db
        .append_message(
            "a",
            "assistant",
            &format!("closed end {}", "y".repeat(8_192)),
        )
        .unwrap();
    harness
        .db
        .append_message("a", "user", "uncompressed tail")
        .unwrap();

    let mut a_read = sse_tool_call("a-read", "read", &serde_json::json!({"path": "note.txt"}));
    a_read.push_str(&sse_completed());
    let mut b_read = sse_tool_call("b-read", "read", &serde_json::json!({"path": "note.txt"}));
    b_read.push_str(&sse_completed());
    let mut a_compress = sse_tool_call(
        "a-compress",
        "compress",
        &serde_json::json!({
            "topic": "closed setup",
            "content": [{
                "startId": start,
                "endId": end,
                "summary": "closed setup is complete"
            }]
        }),
    );
    a_compress.push_str(&sse_completed());
    let (base, hits, requests) = Fake::start_recording(
        vec![
            a_read,
            sse_delta("a first complete") + &sse_completed(),
            b_read,
            sse_delta("b first complete") + &sse_completed(),
            a_compress,
            sse_delta("a compressed") + &sse_completed(),
            sse_delta("b cadence complete") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);

    let a_first = runtime
        .run_turn(params(
            "a",
            "advance A twice",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(a_first.rounds, 2);
    let b_first = runtime
        .run_turn(params(
            "b",
            "advance B twice",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(b_first.rounds, 2);
    let a_second = runtime
        .run_turn(params(
            "a",
            "compress A from the model",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(a_second.rounds, 2);
    assert_eq!(a_second.calls.len(), 1);
    assert_eq!(a_second.calls[0].name, "compress");
    assert_eq!(a_second.calls[0].state, "completed");
    runtime
        .run_turn(params(
            "b",
            "B remains independently due",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();

    assert_eq!(*hits.lock().unwrap(), 7);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 7);
    assert_eq!(
        requests.iter().map(dcp_nudge_count).collect::<Vec<_>>(),
        [1, 0, 1, 0, 1, 0, 1],
        "A and B must keep independent frequency=2 cadence; only A enters cooldown after compress"
    );
    assert_eq!(harness.db.load_compression_blocks("a").unwrap().len(), 1);
    assert!(harness.db.load_compression_blocks("b").unwrap().is_empty());
}

#[tokio::test]
async fn aud20_nudge_cadence_survives_database_and_runtime_restart() {
    let (mut harness, generation) = make_harness(allow_all());
    let dcp = DcpConfig {
        min_context: 1,
        max_context: 1,
        nudge_frequency: 5,
        iteration_threshold: 100,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation.clone(), Vec::new(), dcp.clone());
    runtime.create_session("restart-nudge").unwrap();
    let (base, hits, requests) = Fake::start_recording(
        vec![
            sse_delta("first") + &sse_completed(),
            sse_delta("after restart") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);
    runtime
        .run_turn(params(
            "restart-nudge",
            "first",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    drop(runtime);
    drop(harness.db);
    harness.db = Db::open(harness._data.path()).unwrap();
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.open_session("restart-nudge").unwrap();
    runtime
        .run_turn(params(
            "restart-nudge",
            "second",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(dcp_nudge_count)
            .collect::<Vec<_>>(),
        [1, 0],
        "restart must restore cadence rather than reset and emit immediately"
    );
}

#[tokio::test]
async fn aud19_denied_compress_has_no_schema_anchor_or_nudge() {
    let mut permissions = allow_all();
    permissions.insert("compress".to_string(), Permission::Deny);
    let (harness, generation) = make_harness(permissions);
    let dcp = DcpConfig {
        min_context: 1,
        max_context: 1,
        compress_permission: Some(Permission::Deny),
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.create_session("denied-compress").unwrap();
    let (base, _, requests) =
        Fake::start_recording(vec![sse_delta("done") + &sse_completed()], Duration::ZERO);
    runtime
        .run_turn(params(
            "denied-compress",
            "normal turn",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        !requests[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "compress")
    );
    let input = requests[0]["input"].to_string();
    assert!(!input.contains("DCP context anchors"));
    assert!(!input.contains("DCP reminder"));
}

#[tokio::test]
async fn aud21_turn_protection_preserves_recent_completed_turn_verbatim() {
    let (harness, generation) = make_harness(allow_all());
    let dcp = DcpConfig {
        turn_protection: true,
        turn_protection_turns: 1,
        deduplication: false,
        purge_errors: false,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.create_session("turn-protection").unwrap();
    let recent_user = format!("RECENT_USER_EXACT {}", "u".repeat(8_192));
    let recent_assistant = format!("RECENT_ASSISTANT_EXACT {}", "a".repeat(8_192));
    let start = harness
        .db
        .append_message("turn-protection", "user", &recent_user)
        .unwrap();
    let end = harness
        .db
        .append_message("turn-protection", "assistant", &recent_assistant)
        .unwrap();
    let mut compress = sse_tool_call(
        "turn-protection-compress",
        "compress",
        &serde_json::json!({
            "topic": "recent turn",
            "content": [{
                "startId": start, "endId": end,
                "summary": "recent turn summary"
            }]
        }),
    );
    compress.push_str(&sse_completed());
    let (base, _, requests) = Fake::start_recording(
        vec![compress, sse_delta("protected") + &sse_completed()],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "turn-protection",
            "attempt compression of recent completed turn",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.calls.len(), 1);
    assert!(matches!(
        report.calls[0].state.as_str(),
        "completed" | "no_gain"
    ));
    let requests = requests.lock().unwrap();
    let next = requests[1]["input"].to_string();
    assert!(next.contains("RECENT_USER_EXACT"));
    assert!(next.contains("RECENT_ASSISTANT_EXACT"));
}

#[tokio::test]
async fn aud20_summary_buffer_changes_effective_nudge_threshold() {
    let (harness, generation) = make_harness(allow_all());
    let mut dcp = DcpConfig {
        min_context: 500,
        max_context: 600,
        nudge_frequency: 1,
        summary_buffer: true,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp.clone());
    runtime.create_session("summary-buffer").unwrap();
    let first = harness
        .db
        .append_message("summary-buffer", "user", &"u".repeat(10_000))
        .unwrap();
    let second = harness
        .db
        .append_message("summary-buffer", "assistant", &"a".repeat(10_000))
        .unwrap();
    harness
        .db
        .append_message("summary-buffer", "user", "tail")
        .unwrap();
    oc_adapters::dcp::save_block(
        &harness.db,
        "summary-buffer",
        "buffer",
        &"s".repeat(4_000),
        &first,
        &second,
        &[first.clone(), second.clone()],
    )
    .unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_delta("buffered") + &sse_completed(),
            sse_delta("unbuffered") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);
    runtime
        .run_turn(params(
            "summary-buffer",
            "small request",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    dcp.summary_buffer = false;
    runtime.reload_dcp(dcp).unwrap();
    runtime
        .run_turn(params(
            "summary-buffer",
            "small request two",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    let first = requests[0]["input"].to_string();
    let second = requests[1]["input"].to_string();
    assert!(first.contains("DCP reminder (advisory)"));
    assert!(!first.contains("required before more work"));
    assert!(second.contains("DCP reminder (required before more work)"));
}

#[tokio::test]
async fn aud20_compress_commits_only_eligible_strategy_projection() {
    let (harness, generation) = make_harness(allow_all());
    let mut dcp = DcpConfig {
        deduplication: true,
        purge_errors: true,
        purge_after_turns: 1,
        protected_tools: vec!["bash".to_string()],
        protected_file_patterns: vec!["src/*.rs".to_string()],
        turn_protection: true,
        turn_protection_turns: 1,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp.clone());
    runtime.create_session("strategy").unwrap();
    std::fs::write(harness._project.path().join("note.txt"), "durable note").unwrap();

    let duplicate_args = serde_json::json!({"path": "note.txt"});
    let protected_args = serde_json::json!({"argv": ["/bin/true", "protected"]});
    let error_args = serde_json::json!({"path": format!("/{}", "e".repeat(5_000))});
    let recent_error_args = serde_json::json!({"path": format!("/{}", "r".repeat(5_000))});
    let large_success_args = serde_json::json!({"argv": ["/bin/true", "s".repeat(5_000)]});
    let protected_patch_args = serde_json::json!({
        "patchText": format!(
            "*** Begin Patch\n*** Update File: src/critical.rs\n@@\n-missing\n+{}\n*** End Patch\n",
            "p".repeat(5_000)
        )
    });
    assert!(error_args.to_string().len() > 4_096);
    assert!(large_success_args.to_string().len() > 4_096);
    assert!(protected_patch_args.to_string().len() > 4_096);

    let mut old_batch = String::new();
    old_batch.push_str(&sse_tool_call("dup-read", "read", &duplicate_args));
    old_batch.push_str(&sse_tool_call("protected-1", "bash", &protected_args));
    old_batch.push_str(&sse_tool_call("error-old", "read", &error_args));
    old_batch.push_str(&sse_tool_call("large-success", "bash", &large_success_args));
    old_batch.push_str(&sse_tool_call(
        "protected-file",
        "apply_patch",
        &protected_patch_args,
    ));
    old_batch.push_str(&sse_completed());
    let mut recent_batch = String::new();
    // Provider call IDs are opaque and may repeat in a later turn. Strategy
    // identity must not hide every occurrence merely because one is deduped.
    recent_batch.push_str(&sse_tool_call("dup-read", "read", &duplicate_args));
    recent_batch.push_str(&sse_tool_call("protected-2", "bash", &protected_args));
    recent_batch.push_str(&sse_tool_call("error-recent", "read", &recent_error_args));
    recent_batch.push_str(&sse_completed());
    let mut compress = sse_tool_call(
        "strategy-compress",
        "compress",
        &serde_json::json!({
            "topic": "old tool work",
            "content": [{
                "startId": "m0001", "endId": "m0004",
                "summary": "old tool work completed"
            }]
        }),
    );
    compress.push_str(&sse_completed());
    let mut third_duplicate = sse_tool_call("dup-read", "read", &duplicate_args);
    third_duplicate.push_str(&sse_completed());
    let mut second_compress = sse_tool_call(
        "strategy-compress-2",
        "compress",
        &serde_json::json!({
            "topic": "first strategy pass",
            "content": [{
                "startId": "m0005", "endId": "m0006",
                "summary": "first strategy pass completed"
            }]
        }),
    );
    second_compress.push_str(&sse_completed());
    let (base, hits, requests) = Fake::start_recording(
        vec![
            old_batch,
            sse_delta("old tools complete") + &sse_completed(),
            recent_batch,
            sse_delta("recent tools complete") + &sse_completed(),
            compress,
            sse_delta("strategy projection captured") + &sse_completed(),
            third_duplicate,
            sse_delta("third duplicate captured") + &sse_completed(),
            second_compress,
            sse_delta("second strategy projection captured") + &sse_completed(),
            sse_delta("manual projection captured") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);

    let old = runtime
        .run_turn(params(
            "strategy",
            "seed old typed tools",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(old.calls.len(), 5);
    let recent = runtime
        .run_turn(params(
            "strategy",
            "seed recent typed tools",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(recent.calls.len(), 3);
    let exact_error = harness
        .db
        .list_tool_ops("strategy")
        .unwrap()
        .into_iter()
        .find(|op| op.op.ends_with("-error-old"))
        .and_then(|op| op.output)
        .expect("durable old error output");
    assert!(exact_error.starts_with("error:"));

    runtime
        .run_turn(params(
            "strategy",
            &format!(
                "compress and commit automatic strategies {}",
                "strategy-padding ".repeat(1_000)
            ),
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let third = runtime
        .run_turn(params(
            "strategy",
            "seed a third reused provider call id",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(third.calls.len(), 1);
    runtime
        .run_turn(params(
            "strategy",
            "compress again without occurrence drift",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    dcp.manual_mode = true;
    dcp.automatic_strategies = false;
    runtime.reload_dcp(dcp).unwrap();
    runtime
        .run_turn(params(
            "strategy",
            "capture manual bypass",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();

    assert_eq!(*hits.lock().unwrap(), 11);
    let runs = harness
        .db
        .list_tool_ops("strategy")
        .unwrap()
        .into_iter()
        .filter_map(|op| op.dcp)
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 2);
    assert_eq!(
        (runs[0].ordinal, runs[0].new_tools),
        (1, 2),
        "only hidden duplicate and actual purged input are covered"
    );
    assert_eq!(
        (runs[1].ordinal, runs[1].new_tools),
        (2, 2),
        "reused call ID and newly aged error are new occurrences; inherited pruning is excluded"
    );
    assert_eq!(runs[1].cumulative.prunes, 4);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 11);
    let projected = &requests[5];
    assert_eq!(
        function_item_count(projected, "function_call", "dup-read"),
        1
    );
    assert_eq!(
        function_item_count(projected, "function_call_output", "dup-read"),
        1
    );
    for call_id in [
        "protected-1",
        "error-old",
        "large-success",
        "protected-file",
        "protected-2",
        "error-recent",
    ] {
        assert!(
            function_call(projected, call_id).is_some(),
            "strategy removed eligible call {call_id}"
        );
        assert!(
            function_output(projected, call_id).is_some(),
            "strategy orphaned output {call_id}"
        );
    }
    assert_eq!(
        function_call(projected, "error-old").unwrap()["arguments"],
        serde_json::json!({"purged": "large error input"}).to_string()
    );
    assert_eq!(
        function_call(projected, "error-recent").unwrap()["arguments"],
        recent_error_args.to_string(),
        "turnProtection must prevent purge of recent typed tool input"
    );
    assert_eq!(
        function_output(projected, "error-old"),
        Some(exact_error.as_str())
    );
    assert_eq!(
        function_call(projected, "large-success").unwrap()["arguments"],
        large_success_args.to_string(),
        "large successful arguments must not be purged"
    );
    assert_eq!(
        function_call(projected, "protected-file").unwrap()["arguments"],
        protected_patch_args.to_string(),
        "protectedFilePatterns must inspect typed apply_patch paths"
    );
    assert_eq!(
        function_call(projected, "dup-read").unwrap()["arguments"],
        duplicate_args.to_string()
    );
    assert_eq!(
        function_call(projected, "protected-1").unwrap()["arguments"],
        protected_args.to_string()
    );
    assert_eq!(
        function_call(projected, "protected-2").unwrap()["arguments"],
        protected_args.to_string()
    );

    let projected_again = &requests[9];
    assert_eq!(
        function_item_count(projected_again, "function_call", "dup-read"),
        1,
        "a second strategy transaction must keep only the newest reused call ID occurrence"
    );
    assert_eq!(
        function_item_count(projected_again, "function_call_output", "dup-read"),
        1
    );

    let manual = &requests[10];
    assert_eq!(function_item_count(manual, "function_call", "dup-read"), 1);
    assert_eq!(
        function_item_count(manual, "function_call_output", "dup-read"),
        1
    );
    for (call_id, arguments) in [
        ("protected-1", protected_args.to_string()),
        (
            "error-old",
            serde_json::json!({"purged": "large error input"}).to_string(),
        ),
        ("large-success", large_success_args.to_string()),
        ("protected-file", protected_patch_args.to_string()),
        ("dup-read", duplicate_args.to_string()),
        ("protected-2", protected_args.to_string()),
        (
            "error-recent",
            serde_json::json!({"purged": "large error input"}).to_string(),
        ),
    ] {
        assert_eq!(
            function_call(manual, call_id).map(|call| &call["arguments"]),
            Some(&serde_json::Value::String(arguments)),
            "manual mode changed committed projection for {call_id}"
        );
        assert!(
            function_output(manual, call_id).is_some(),
            "manual mode removed output {call_id}"
        );
    }
    assert_eq!(
        function_output(manual, "error-old"),
        Some(exact_error.as_str())
    );
}

#[tokio::test]
async fn aud21_model_compress_preserves_complete_tool_graph_without_replay() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("graph").unwrap();
    let mut effect = sse_tool_call(
        "graph-effect",
        "bash",
        &serde_json::json!({
            "argv": ["/bin/sh", "-c", "printf 'once\\n' >> graph-effects"]
        }),
    );
    effect.push_str(&sse_completed());
    let (base, seed_hits, _) = Fake::start_recording(
        vec![
            effect,
            sse_delta(&format!("effect complete {}", "padding ".repeat(2_000))) + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);
    let seeded = runtime
        .run_turn(params(
            "graph",
            &format!(
                "perform one durable effect {}",
                "request-padding ".repeat(2_000)
            ),
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(seeded.calls.len(), 1);
    assert_eq!(seeded.calls[0].state, "completed");
    let history = harness.db.read_history_full("graph").unwrap();
    assert_eq!(history.len(), 2);

    let mut compress = sse_tool_call(
        "compress-graph",
        "compress",
        &serde_json::json!({
            "topic": "unsafe graph range",
            "content": [{
                "startId": history[0].0,
                "endId": history[1].0,
                "summary": "the durable effect completed once"
            }]
        }),
    );
    compress.push_str(&sse_completed());
    let (compress_base, compress_hits, requests) = Fake::start_recording(
        vec![compress, sse_delta("refusal handled") + &sse_completed()],
        Duration::ZERO,
    );
    let mut events = Vec::new();
    let report = runtime
        .run_turn_with_tool_events(
            params(
                "graph",
                "compress the completed tool turn",
                &harness,
                provider_of(&compress_base),
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
    assert_eq!(report.calls[0].name, "compress");
    assert_eq!(report.calls[0].state, "completed");
    assert!(events.iter().any(|event|matches!(event,ToolCallEvent::Started { name,dcp_topic:Some(topic),.. } if name=="compress" && topic=="unsafe graph range")));
    let requests = requests.lock().unwrap();
    assert!(
        function_output(&requests[1], "compress-graph")
            .is_some_and(|output| output.contains("\"status\":\"compressed\""))
    );
    assert!(function_call(&requests[1], "graph-effect").is_some());
    assert!(function_output(&requests[1], "graph-effect").is_some());
    assert!(harness.db.load_compression_blocks("graph").unwrap().len() == 1);
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("graph-effects")).unwrap(),
        "once\n"
    );
    let operations = harness.db.list_tool_ops("graph").unwrap();
    assert_eq!(
        operations
            .iter()
            .find(|op| op.name == "compress")
            .unwrap()
            .dcp_topic
            .as_deref(),
        Some("unsafe graph range")
    );
    let snapshot = operations
        .iter()
        .find(|op| op.name == "compress")
        .unwrap()
        .dcp
        .as_ref()
        .unwrap();
    assert_eq!(snapshot.ordinal, 1);
    assert_eq!(
        (snapshot.new_messages, snapshot.new_tools),
        (2, 0),
        "retained function pairs are not compressed tool occurrences"
    );
    let estimate = |text: &str| (text.encode_utf16().count() as u64 + 2) / 4;
    assert_eq!(
        snapshot.removed,
        history
            .iter()
            .map(|(_, _, text)| estimate(text))
            .sum::<u64>()
    );
    assert_eq!(
        snapshot.summary,
        estimate("the durable effect completed once")
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e,ToolCallEvent::Finished { dcp:Some(run), .. } if run==snapshot))
    );
    let request_tokens = |request: &serde_json::Value| {
        let mut total = 0u64;
        for item in request["input"].as_array().unwrap() {
            if item["call_id"] == "compress-graph" {
                continue;
            }
            if let Some(parts) = item["content"].as_array() {
                for part in parts {
                    if let Some(text) = part["text"].as_str() {
                        // The measured projection excludes generated anchor-lane
                        // instructions, which are rebuilt independently per request.
                        if !text.starts_with("DCP context anchors in order.") {
                            total += estimate(text);
                        }
                    }
                }
            }
            if let Some(arguments) = item["arguments"].as_str() {
                total += estimate(arguments);
            }
            if let Some(output) = item["output"].as_str() {
                total += estimate(output);
            }
        }
        total
    };
    assert!(request_tokens(&requests[1]) < request_tokens(&requests[0]));
    assert_eq!(
        snapshot.net_saved,
        request_tokens(&requests[0]) - request_tokens(&requests[1])
    );
    let raw_after = harness.db.read_history_full("graph").unwrap();
    assert_eq!(&raw_after[..history.len()], history.as_slice());
    assert_eq!(raw_after.len(), 4, "DCP metadata adds no history message");
    assert_eq!(
        operations.iter().filter(|op| op.name == "bash").count(),
        1,
        "compression refusal replayed the prior side effect"
    );
    assert_eq!(*seed_hits.lock().unwrap(), 2);
    assert_eq!(
        *compress_hits.lock().unwrap(),
        2,
        "compression must return one structured output and then continue"
    );
}

/// DTO extension (iteration 3a): the runtime forwards provider reasoning
#[tokio::test]
async fn vis38_failed_no_gain_and_cancel_never_publish_success_accounting() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("negative-dcp").unwrap();
    let first = harness
        .db
        .append_message("negative-dcp", "user", &"raw content ".repeat(200))
        .unwrap();
    let second = harness
        .db
        .append_message("negative-dcp", "assistant", &"answer ".repeat(200))
        .unwrap();
    let call = |id: &str, start: &str, end: &str, summary: &str| {
        sse_tool_call(
            id,
            "compress",
            &serde_json::json!({"topic":"negative outcome","content":[{"startId":start,"endId":end,"summary":summary}]}),
        ) + &sse_completed()
    };
    let (base, _, _) = Fake::start_recording(
        vec![
            call("no-gain", &first, &second, &"too much summary ".repeat(400)),
            sse_delta("no gain handled") + &sse_completed(),
            call("failed", "missing", &second, "short"),
            sse_delta("failure handled") + &sse_completed(),
            call("cancelled", &first, &second, "short"),
        ],
        Duration::ZERO,
    );
    for (prompt, state) in [
        ("attempt no gain", "no_gain"),
        ("attempt failure", "failed"),
    ] {
        let report = runtime
            .run_turn(params(
                "negative-dcp",
                prompt,
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(report.calls[0].state, state);
    }
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    runtime
        .run_turn_with_tool_events(
            params(
                "negative-dcp",
                "attempt cancellation",
                &harness,
                provider_of(&base),
                &cancel,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if matches!(event,ToolCallEvent::Started { name,.. } if name=="compress") {
                    cancel.store(true, Ordering::Release);
                }
                events.push(event.clone());
            },
        )
        .await
        .unwrap();
    assert!(
        events.iter().any(
            |e| matches!(e,ToolCallEvent::Finished { state,dcp:None,.. } if state=="cancelled")
        )
    );
    assert!(harness.db.dcp_accounting("negative-dcp").unwrap().is_none());
    assert!(
        harness
            .db
            .load_compression_blocks("negative-dcp")
            .unwrap()
            .is_empty()
    );
    assert!(
        harness
            .db
            .list_tool_ops("negative-dcp")
            .unwrap()
            .iter()
            .all(|op| op.dcp.is_none())
    );
}

#[tokio::test]
async fn provider_context_usage_survives_missing_round_usage_and_restart() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let tool = sse_tool_call(
        "missing",
        "read",
        &serde_json::json!({"path":"missing.txt"}),
    );
    let (base, hits) = Fake::start(
        vec![
            tool + sse_completed_without_usage(),
            sse_delta("answer") + &sse_completed_usage(6000, 763),
            sse_tool_call("known", "read", &serde_json::json!({"path":"missing.txt"}))
                + &sse_completed_usage(4500, 21),
            sse_delta("later") + sse_completed_without_usage(),
            sse_tool_call(
                "unfinished",
                "read",
                &serde_json::json!({"path":"missing.txt"}),
            ) + &sse_completed_usage(7000, 34),
            sse_tool_call(
                "cancel-after-tool",
                "read",
                &serde_json::json!({"path":"missing.txt"}),
            ) + &sse_completed_usage(8000, 45),
        ],
        Duration::ZERO,
    );
    let fresh = runtime
        .run_fresh_turn_with_tool_events(
            params("context", "first", &harness, provider_of(&base), &NO_CANCEL),
            None,
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(fresh.status, TurnStatus::Completed);
    assert_eq!(fresh.rounds, 2);
    assert_eq!(fresh.usage, None, "missing billed tool round is unknown");
    assert_eq!(fresh.context_usage, Some((6000, 763)));

    let next = runtime
        .run_turn_with_tool_events(
            params(
                "context",
                "second",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(next.status, TurnStatus::Completed);
    assert_eq!(next.rounds, 2);
    assert_eq!(next.usage, None);
    assert_eq!(
        next.context_usage,
        Some((4500, 21)),
        "a missing final round must not erase a known pair"
    );

    let mut incomplete_params =
        params("context", "third", &harness, provider_of(&base), &NO_CANCEL);
    incomplete_params.max_rounds = 1;
    let incomplete = runtime.run_turn(incomplete_params).await.unwrap();
    assert_eq!(incomplete.status, TurnStatus::Incomplete);
    assert_eq!(incomplete.usage, Some((7000, 34)));
    assert_eq!(incomplete.context_usage, Some((7000, 34)));

    let cancelled_flag = AtomicBool::new(false);
    let cancelled = runtime
        .run_turn_with_tool_events(
            params(
                "context",
                "fourth",
                &harness,
                provider_of(&base),
                &cancelled_flag,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if matches!(event, ToolCallEvent::Finished { .. }) {
                    cancelled_flag.store(true, Ordering::Relaxed);
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(cancelled.status, TurnStatus::Cancelled);
    assert_eq!(cancelled.usage, Some((8000, 45)));
    assert_eq!(cancelled.context_usage, Some((8000, 45)));
    assert_eq!(*hits.lock().unwrap(), 6);

    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    for (id, context, expected_status) in [
        (fresh.turn_id, [6000, 763], "completed"),
        (next.turn_id, [4500, 21], "completed"),
        (incomplete.turn_id, [7000, 34], "incomplete"),
        (cancelled.turn_id, [8000, 45], "cancelled"),
    ] {
        let (status, result) = reopened.turn_result(&id).unwrap();
        assert_eq!(status, expected_status);
        let result: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(
            result["display"]["context_usage"],
            serde_json::json!(context)
        );
        if status == "completed" {
            assert!(
                result["display"].get("usage").is_none(),
                "unknown billed usage must not reappear"
            );
        }
    }
}
