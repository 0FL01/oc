use super::*;

#[tokio::test]
async fn hot02_tool_notification_changes_the_next_same_task_catalog_not_the_issued_batch() {
    use serde_json::json;
    use tokio::io::AsyncWriteExt as _;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    let script = r#"
import json, os, pathlib, sys
label = sys.argv[1]
log = pathlib.Path(label + '.events')
log.write_text('pid %d\n' % os.getpid())
changed = False
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message: continue
    method = message['method']
    with log.open('a') as file: file.write(method + '\n')
    if method == 'initialize':
        result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{'listChanged':True}},'serverInfo':{'name':'notification-fixture','version':'1'}}
    elif method == 'tools/list':
        name = ('new' if changed else 'old') if label == 'dirty' else 'ping'
        result = {'tools':[{'name':name,'inputSchema':{'type':'object'}}]}
    else:
        if label == 'dirty' and not changed:
            changed = True
            # Deliver the notification before the call reply, so the next request
            # boundary has an acknowledged dirty catalog without timing sleeps.
            print(json.dumps({'jsonrpc':'2.0','method':'notifications/tools/list_changed'}), flush=True)
        result = {'content':[{'type':'text','text':'NOTIFICATION_ISSUED_RESULT'}]}
    print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}), flush=True)
"#;
    owner.update_execution(|view| {
        for label in ["dirty", "stable"] {
            view.generation.config.mcp.insert(
                label.into(),
                crate::config::McpEntry {
                    kind: "local".into(),
                    command: vec![
                        "/usr/bin/python3".into(),
                        "-u".into(),
                        "-c".into(),
                        script.into(),
                        label.into(),
                    ],
                    cwd: Some(project.path().to_string_lossy().into_owned()),
                    resource_admitted: true,
                    codemode: Some(false),
                    ..Default::default()
                },
            );
        }
    });
    owner.start_mcp().unwrap();
    let supervisor = owner.mcp_owner();
    tokio::time::timeout(Duration::from_secs(3), async {
        while supervisor.request_view().unwrap().entries.len() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        timeout: None,
        chunk_timeout_ms: 5000,
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        set_cache_key: false,
        wire: Default::default(),
    };
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([(
            "m".into(),
            json!({"limit":{"context":500000,"output":8000}}),
        )]),
    };
    let peer = tokio::spawn(async move {
        let (mut socket, first) = binding_replay_tests::request(&listener).await;
        let tools = first["tools"].as_array().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == "dirty__old"));
        assert!(tools.iter().any(|tool| tool["name"] == "stable__ping"));
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","id":"old-a","call_id":"issued-old-a","name":"dirty__old","arguments":"{}"},{"type":"function_call","id":"old-b","call_id":"issued-old-b","name":"dirty__old","arguments":"{}"}]}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let (mut socket, next) = binding_replay_tests::request(&listener).await;
        let adopted = next["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "dirty__new");
        let still_old = next["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "dirty__old");
        let stable = next["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "stable__ping");
        let settled = next["input"]
            .to_string()
            .contains("NOTIFICATION_ISSUED_RESULT");
        assert!(next["input"].to_string().contains("issued-old-a"));
        assert!(next["input"].to_string().contains("issued-old-b"));
        // Complete even on the red path so owner shutdown joins both stdio peers.
        binding_replay_tests::respond(
            &mut socket,
            crate::provider::protocol::Protocol::Responses,
            false,
        )
        .await;
        (adopted, still_old, stable, settled)
    });
    let cancel = AtomicBool::new(false);
    let report = tokio::time::timeout(
        Duration::from_secs(8),
        owner.run_turn(TurnParams {
            session: "s".into(),
            prompt: "continue after the issued tool changes its catalog".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 1000,
            provider,
            cancel: &cancel,
        }),
    )
    .await
    .unwrap();
    let observed = peer.await;
    owner.shutdown_mcp().await.unwrap();
    let (adopted, still_old, stable, settled) = observed.unwrap();
    assert!(
        adopted && !still_old,
        "notification was not admitted at the next same-task request"
    );
    assert!(stable && settled);
    assert_eq!(report.unwrap().status, TurnStatus::Completed);
    assert!(Arc::ptr_eq(&supervisor, &owner.mcp_owner()));
    let operations = db.list_tool_ops("s").unwrap();
    assert_eq!(operations.len(), 2);
    assert!(
        operations
            .iter()
            .all(|operation| operation.state == "completed")
    );
    for (label, lists) in [("dirty", 2), ("stable", 1)] {
        let events =
            std::fs::read_to_string(project.path().join(format!("{label}.events"))).unwrap();
        assert_eq!(
            events.lines().filter(|line| *line == "initialize").count(),
            1
        );
        assert_eq!(
            events.lines().filter(|line| *line == "tools/list").count(),
            lists
        );
        let pid = events.lines().next().unwrap().strip_prefix("pid ").unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[tokio::test]
async fn hot01_mcp_batch_releases_before_same_task_replacement_and_next_catalog() {
    use serde_json::json;
    use tokio::io::AsyncWriteExt as _;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("note.txt"), "SETTLED_LOCAL_NOTE").unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    let script = r#"
import json, os, pathlib, sys
label = sys.argv[1]
pathlib.Path(label + '.pid').write_text(str(os.getpid()))
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message: continue
    if message['method'] == 'initialize': result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{}},'serverInfo':{'name':'lease-fixture','version':'1'}}
    elif message['method'] == 'tools/list': result = {'tools':[{'name':label,'inputSchema':{'type':'object'}}]}
    else: result = {'content':[{'type':'text','text':'RESULT_FROM_' + label.upper()}]}
    print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}), flush=True)
"#;
    let entry = |label: &str| crate::config::McpEntry {
        kind: "local".into(),
        command: vec![
            "/usr/bin/python3".into(),
            "-u".into(),
            "-c".into(),
            script.into(),
            label.into(),
        ],
        cwd: Some(project.path().to_string_lossy().into_owned()),
        resource_admitted: true,
        codemode: Some(false),
        ..Default::default()
    };
    owner.update_execution(|view| {
        view.generation
            .config
            .mcp
            .insert("peer".into(), entry("first"));
    });
    owner.start_mcp().unwrap();
    let supervisor = owner.mcp_owner();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if supervisor
                .request_view()
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.tool == "first")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let first_pid = std::fs::read_to_string(project.path().join("first.pid")).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        timeout: None,
        chunk_timeout_ms: 5000,
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        set_cache_key: false,
        wire: Default::default(),
    };
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([(
            "m".into(),
            json!({"limit":{"context":500000,"output":8000}}),
        )]),
    };
    let (entered, held) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let (followup, next_request) = tokio::sync::oneshot::channel();
    let (replacement, ready) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        assert!(
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "peer__first")
        );
        entered.send(()).unwrap();
        released.await.unwrap();
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","id":"old","call_id":"issued-old","name":"peer__first","arguments":"{}"}]}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        assert!(request["input"].to_string().contains("RESULT_FROM_FIRST"));
        assert!(request["input"].to_string().contains("ATOMIC_G2_FRESH"));
        followup.send(()).unwrap();
        if ready.await.unwrap() {
            binding_replay_tests::respond(
                &mut socket,
                crate::provider::protocol::Protocol::Responses,
                true,
            )
            .await;
            let (mut socket, request) = binding_replay_tests::request(&listener).await;
            let tools = request["tools"].as_array().unwrap();
            assert!(tools.iter().any(|tool| tool["name"] == "peer__second"));
            assert!(tools.iter().all(|tool| tool["name"] != "peer__first"));
            assert!(request["input"].to_string().contains("SETTLED_LOCAL_NOTE"));
            assert!(request["input"].to_string().contains("ATOMIC_G2_FRESH"));
            binding_replay_tests::respond(
                &mut socket,
                crate::provider::protocol::Protocol::Responses,
                false,
            )
            .await;
        } else {
            // Drain the turn even on the red path, then join every owned process.
            binding_replay_tests::respond(
                &mut socket,
                crate::provider::protocol::Protocol::Responses,
                false,
            )
            .await;
        }
    });
    let cancel = AtomicBool::new(false);
    let turn = owner.run_turn(TurnParams {
        session: "s".into(),
        prompt: "continue across service replacement".into(),
        invocation: None,
        catalog: &catalog,
        model_id: "m".into(),
        variant: None,
        max_output: 1000,
        provider,
        cancel: &cancel,
    });
    let publication = async {
        held.await.unwrap();
        let previous = owner.current.read().unwrap().clone();
        let mut next = previous.as_ref().clone();
        next.generation.id = 2;
        next.generation
            .config
            .mcp
            .insert("peer".into(), entry("second"));
        next.workspace.fixed_input =
            vec![InputItem::message(InputRole::Developer, "ATOMIC_G2_FRESH")];
        next.workspace.agent_digest = Some("G2_ATOMIC_PROFILE".into());
        supervisor
            .reconcile(
                next.config.clone(),
                2,
                None,
                false,
                Some(owner.source_commit(previous, next)),
            )
            .await
            .unwrap();
        release.send(()).unwrap();
        next_request.await.unwrap();
        let progressed = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if supervisor
                    .request_view()
                    .unwrap()
                    .entries
                    .iter()
                    .any(|entry| entry.tool == "second")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .is_ok();
        if progressed {
            assert!(!std::path::Path::new(&format!("/proc/{}", first_pid.trim())).exists());
        }
        replacement.send(progressed).unwrap();
        progressed
    };
    let (report, progressed) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(turn, publication)
    })
    .await
    .unwrap();
    let peer = peer.await;
    owner.shutdown_mcp().await.unwrap();
    peer.unwrap();
    assert!(
        progressed,
        "initial MCP view pinned the old physical client beyond its issued batch"
    );
    let report = report.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert!(Arc::ptr_eq(&supervisor, &owner.mcp_owner()));
    let ops = db.list_tool_ops("s").unwrap();
    assert_eq!(ops.len(), 2);
    assert!(ops.iter().all(|op| op.state == "completed"));
    let raw = db.turn_result(&report.turn_id).unwrap().1.unwrap();
    let log: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        log["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["config_generation"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 2]
    );
    for label in ["first", "second"] {
        let pid = std::fs::read_to_string(project.path().join(format!("{label}.pid"))).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
    }
}

fn workspace(owner: &Runtime<'_>, label: &str, permission: Permission) {
    owner
        .publish_workspace(
            Some(label),
            &format!("{label} instructions"),
            vec![(
                "manual".into(),
                format!("---\nname: manual\ndescription: {label} metadata\n---\n{label} body\n"),
            )],
            BTreeMap::new(),
            Some(label.into()),
            Some("build".into()),
            None,
            BTreeMap::from([("read".into(), permission)]),
            Default::default(),
            Default::default(),
        )
        .unwrap();
}

#[test]
fn hot01_captured_publication_keeps_its_workspace_and_policy() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    workspace(&owner, "G1", Permission::Allow);
    let captured = owner.current.read().unwrap().clone();
    let issuing = owner.primary_lane(&captured);

    workspace(&owner, "G2", Permission::Deny);
    owner
        .reload_dcp(DcpConfig {
            debug: !captured.dcp_config.debug,
            ..captured.dcp_config.clone()
        })
        .unwrap();
    owner
        .publish_dcp_protection(ProtectedSpec {
            protect_tags: !captured.dcp_protected.protect_tags,
            ..captured.dcp_protected.clone()
        })
        .unwrap();
    owner
        .publish_provider_state(
            crate::composition::ProviderState::new("fixture", "G2", false, false),
            None,
        )
        .unwrap();
    let still_issuing = owner.primary_lane(&captured);
    assert_eq!(still_issuing.fixed_input, issuing.fixed_input);
    assert_eq!(still_issuing.agent_digest, issuing.agent_digest);
    assert_eq!(
        RuntimePolicy::with_rules(&still_issuing.permissions, &still_issuing.permission_rules)
            .effect("read", "file"),
        Permission::Allow,
    );

    let current = owner.current.read().unwrap().clone();
    assert!(!Arc::ptr_eq(&captured, &current));
    assert!(
        captured.workspace.skills.entries["manual"]
            .body
            .contains("G1 body")
    );
    assert!(
        current.workspace.skills.entries["manual"]
            .body
            .contains("G2 body")
    );
    assert_ne!(captured.dcp_config.debug, current.dcp_config.debug);
    assert_ne!(
        captured.dcp_protected.protect_tags,
        current.dcp_protected.protect_tags
    );
    assert!(captured.provider_state.is_none());
    assert!(current.provider_state.is_some());
    let fresh = owner.primary_lane(&current);
    assert_ne!(fresh.fixed_input, issuing.fixed_input);
    assert_eq!(fresh.agent_digest.as_deref(), Some("G2"));
    assert_eq!(
        RuntimePolicy::with_rules(&fresh.permissions, &fresh.permission_rules)
            .effect("read", "file"),
        Permission::Deny,
    );
    assert!(db.list_sessions().unwrap().is_empty());
}

#[test]
fn hot01_profile_history_keeps_closed_pairs_and_each_request_origin() {
    use oc_core::queries::{ModelRef, RequestIdentity, RequestProfile};
    use serde_json::json;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    owner.create_session("profiles").unwrap();
    let accepted = db
        .accept_turn(
            "profile-turn",
            "profiles",
            "task",
            "task",
            &ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
        )
        .unwrap();
    let binding = ResponsesConfig {
        base_url: "https://example.test/v1".into(),
        api_key: "synthetic".into(),
        timeout: None,
        chunk_timeout_ms: 1000,
        connect_timeout: Duration::from_secs(1),
        allow_private: false,
        headers: BTreeMap::new(),
        set_cache_key: false,
        wire: Default::default(),
    }
    .provenance("fixture", "m")
    .unwrap();
    let mut log = TurnLog::new("profile-turn", "m", "fixture");
    log.user_message = Some(accepted.user_message);
    log.binding = Some(binding.clone());
    log.agent_digest = Some("profile-a".into());
    log.input.push(InputItem::message(InputRole::User, "task"));
    for (index, digest) in [(1, "profile-a"), (2, "profile-b")] {
        log.requests.push(RequestIdentity {
            config_generation: Some(index),
            profile: Some(RequestProfile {
                agent_digest: Some(digest.into()),
            }),
            binding: Some(binding.clone()),
            model: ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
            model_label: "m".into(),
            span: format!("span-{index}"),
            input_start: log.input.len(),
            context_limit: 1000,
            input_limit: 1000,
            output_limit: 100,
            estimated_input: 1,
            dcp_min_context: 1,
            dcp_max_context: 1000,
            tool_fingerprint: "tools".into(),
            context_fingerprint: "context".into(),
        });
        log.input.extend([
            InputItem::ProviderOutput(json!({"type":"reasoning","id":format!("opaque-{index}"),"encrypted_content":format!("OPAQUE_PROFILE_{index}"),"summary":[{"text":format!("public-{index}")}]})),
            InputItem::ProviderOutput(json!({"type":"function_call","id":format!("opaque-call-{index}"),"call_id":format!("call-{index}"),"name":"read","arguments":"{}"})),
            InputItem::FunctionCallOutput { call_id: format!("call-{index}"), output: format!("SETTLED_PROFILE_{index}") },
        ]);
    }
    let original = log.to_json();
    assert_eq!(log.active_agent_digest(), Some("profile-b"));
    let profile = RequestProfile {
        agent_digest: Some("profile-b".into()),
    };
    let assert_projection = |input: &[InputItem]| {
        let raw = serde_json::to_string(input).unwrap();
        assert!(
            raw.contains("SETTLED_PROFILE_1") && raw.contains("SETTLED_PROFILE_2"),
            "{raw}"
        );
        assert!(
            raw.contains("OPAQUE_PROFILE_2") && !raw.contains("OPAQUE_PROFILE_1"),
            "{raw}"
        );
        assert_eq!(
            input.iter().filter(|i| i.call_output().is_some()).count(),
            2
        );
        assert_eq!(
            input
                .iter()
                .filter(
                    |i| matches!(i, InputItem::ProviderOutput(v) if v["type"] == "function_call")
                )
                .count(),
            2
        );
    };
    assert_projection(&log.input_for_bound("m", "fixture", Some(&binding), Some(&profile)));
    let mut default_profile = log.clone();
    default_profile.requests.last_mut().unwrap().profile = Some(RequestProfile::default());
    assert_eq!(default_profile.active_agent_digest(), None);
    assert_projection(&default_profile.input_for_bound(
        "m",
        "fixture",
        Some(&binding),
        Some(&RequestProfile::default()),
    ));
    let mut legacy = original.clone();
    for receipt in legacy["requests"].as_array_mut().unwrap() {
        receipt.as_object_mut().unwrap().remove("profile");
    }
    let legacy = TurnLog::from_json(&legacy).unwrap();
    assert_eq!(legacy.active_agent_digest(), Some("profile-a"));
    let legacy_input = legacy.input_for_bound("m", "fixture", Some(&binding), Some(&profile));
    assert!(
        !serde_json::to_string(&legacy_input)
            .unwrap()
            .contains("OPAQUE_PROFILE_")
    );
    assert_eq!(
        legacy_input
            .iter()
            .filter(|i| i.call_output().is_some())
            .count(),
        2
    );
    let selected = log.selected_closed_groups(|_| true).unwrap();
    assert_projection(&selected.input_for_bound("m", "fixture", Some(&binding), Some(&profile)));
    let hot = TurnLog::from_json(
        &log.current_working_checkpoint("LATEST_HOT", log.closed_counts(), |_| true)
            .unwrap(),
    )
    .unwrap();
    assert_projection(&hot.input_for_bound("m", "fixture", Some(&binding), Some(&profile)));
    assert_eq!(log.to_json(), original);
    db.finish_turn("profile-turn", "completed", Some(&original.to_string()))
        .unwrap();
    let projection = owner.active_projection("profiles").unwrap();
    assert_projection(
        &owner
            .wire_history_bound(
                "profiles",
                &projection.projected,
                &projection.blocks,
                "m",
                "fixture",
                Some("profile-b"),
                projection.after_seq,
                Some(&binding),
            )
            .unwrap(),
    );
    assert_projection(
        &owner
            .wire_history_view(
                "profiles",
                &projection.projected,
                &projection.blocks,
                "m",
                "fixture",
                Some("profile-b"),
                projection.after_seq,
                Some(&binding),
                false,
            )
            .unwrap(),
    );
    assert!(db.list_tool_ops("profiles").unwrap().is_empty());
}

#[test]
fn hot01_child_source_publication_is_shared_but_launch_authority_is_not() {
    use serde_json::json;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    let agent = SubagentAgent {
        id: "worker".into(),
        description: "worker".into(),
        primary: false,
        model: None,
        variant: None,
        prompt: "G1_CHILD_PROMPT".into(),
        permissions: BTreeMap::from([("read".into(), Permission::Ask)]),
        permission_rules: Default::default(),
        hidden: false,
        digest: Some("g1".into()),
        request: Default::default(),
        color: None,
    };
    owner
        .publish_subagents(Some(SubagentCatalog {
            agents: BTreeMap::from([("worker".into(), agent.clone())]),
            depth_limit: 4,
        }))
        .unwrap();
    owner.update_execution(|view| {
        view.generation.config.permission_rules = crate::permissions::PermissionRules::from_config(
            &json!({"permission":{"read":{"*":"allow","locked.txt":"deny"}}}),
        )
        .unwrap();
        view.generation.config.mcp.insert(
            "service".into(),
            serde_json::from_value(json!({"type":"remote","url":"https://example.test/launch"}))
                .unwrap(),
        );
    });
    let issued = owner.current.read().unwrap().clone();
    let parent = owner.primary_lane(&issued);
    let provider = ResponsesConfig {
        base_url: "https://example.test/source".into(),
        api_key: "SYNTHETIC_CHILD_KEY".into(),
        timeout: None,
        chunk_timeout_ms: 3000,
        connect_timeout: Duration::from_secs(1),
        allow_private: false,
        headers: BTreeMap::new(),
        set_cache_key: false,
        wire: Default::default(),
    };
    let child = owner.owned_child_for(&issued, &agent, &parent, "child-op", "fixture", &provider);
    let launch = child.child_launch.as_ref().unwrap();
    assert!(Arc::ptr_eq(&owner.current, &child.current));
    let root_lease = owner.begin_active().unwrap();
    assert!(!child.active.load(Ordering::Acquire));
    owner.update_execution(|view| {
        view.generation.id = 2;
        view.generation
            .config
            .permissions
            .insert("shell".into(), Permission::Allow);
        view.generation.config.permission_rules = crate::permissions::PermissionRules::from_config(
            &json!({"permission":{"read":"allow","shell":"allow"}}),
        )
        .unwrap();
        let agent = view
            .workspace
            .subagents
            .as_mut()
            .unwrap()
            .agents
            .get_mut("worker")
            .unwrap();
        agent.prompt = "G2_CHILD_PROMPT".into();
        agent.permissions.insert("read".into(), Permission::Allow);
        agent.permissions.insert("shell".into(), Permission::Allow);
        let mut introduced = agent.clone();
        introduced.id = "introduced".into();
        let catalog = view.workspace.subagents.as_mut().unwrap();
        catalog.depth_limit = 99;
        catalog.agents.insert(introduced.id.clone(), introduced);
    });
    let current = child.current.read().unwrap().clone();
    let lane = child.child_request_lane(&current, launch).unwrap();
    assert!(lane.fixed_input.iter().any(|item| {
        serde_json::to_string(item)
            .unwrap()
            .contains("G2_CHILD_PROMPT")
    }));
    let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules);
    assert_eq!(policy.effect("read", "note.txt"), Permission::Ask);
    assert_eq!(policy.effect("read", "locked.txt"), Permission::Deny);
    assert_eq!(policy.effect("shell", "echo unexpected"), Permission::Deny);
    let nested_catalog = launch.subagents(&current).unwrap();
    assert_eq!(nested_catalog.depth_limit, 4);
    assert!(!nested_catalog.agents.contains_key("introduced"));
    launch
        .admit_provider("fixture", "m", None, &provider)
        .unwrap();
    let mut changed = provider.clone();
    changed.base_url = "https://example.test/other".into();
    assert!(
        launch
            .admit_provider("fixture", "m", None, &changed)
            .is_err()
    );
    changed = provider.clone();
    changed.api_key = "ANOTHER_CHILD_ACCOUNT".into();
    assert!(
        launch
            .admit_provider("fixture", "m", None, &changed)
            .is_err()
    );
    assert!(
        launch
            .admit_provider("foreign", "m", None, &provider)
            .is_err()
    );
    let nested = child.owned_child_for(
        &current,
        &nested_catalog.agents["worker"],
        &lane,
        "grandchild-op",
        "fixture",
        &changed,
    );
    assert!(
        nested
            .child_launch
            .as_ref()
            .unwrap()
            .admit_provider("fixture", "m", None, &changed)
            .is_err(),
        "a new delegation cannot discard an ancestor's credential ceiling"
    );
    let mut attached =
        super::super::mcp::McpGeneration::empty(2, Arc::new(tokio::sync::Notify::new()));
    attached.entries = crate::mcp_remote::map_registry(
        "service",
        vec![("lookup".into(), None, json!({"type":"object"}))],
    )
    .unwrap();
    assert_eq!(
        attached
            .within_child(&current.config, &launch.published.config)
            .entries
            .len(),
        1
    );
    let mut replacement = current.config.clone();
    replacement.mcp.get_mut("service").unwrap().timeout = Some(500);
    replacement.mcp.get_mut("service").unwrap().codemode = Some(false);
    assert_eq!(
        attached
            .within_child(&replacement, &launch.published.config)
            .entries
            .len(),
        1
    );
    replacement = current.config.clone();
    replacement.mcp.get_mut("service").unwrap().url = Some("https://example.test/other".into());
    assert!(
        attached
            .within_child(&replacement, &launch.published.config)
            .entries
            .is_empty()
    );
    let mut nested_source = current.as_ref().clone();
    nested_source.generation.config = replacement.clone();
    let nested = child.owned_child_for(
        &nested_source,
        &nested_catalog.agents["worker"],
        &lane,
        "grandchild-service-op",
        "fixture",
        &provider,
    );
    assert!(
        nested
            .child_launch
            .as_ref()
            .unwrap()
            .mcp_view(&attached, &replacement)
            .entries
            .is_empty(),
        "a same-name service replacement cannot be laundered through a grandchild"
    );
    replacement = current.config.clone();
    replacement
        .mcp
        .get_mut("service")
        .unwrap()
        .headers
        .insert("authorization".into(), "Bearer new-domain".into());
    assert!(
        attached
            .within_child(&replacement, &launch.published.config)
            .entries
            .is_empty()
    );
    owner.update_execution(|view| {
        view.workspace.subagents.as_mut().unwrap().agents.clear();
    });
    assert!(
        child
            .child_request_lane(&child.current.read().unwrap(), launch)
            .is_err()
    );
    assert_eq!(launch.published.id, 1);
    drop(root_lease);
    assert!(db.list_sessions().unwrap().is_empty());
}

#[tokio::test]
async fn hot01_child_fresh_requests_intersect_launch_and_route_issued_approvals() {
    use oc_core::approval::{ApprovalDecision, ApprovalReply};
    use serde_json::json;
    use tokio::io::AsyncWriteExt as _;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("note.txt"),
        "first\nsecond\nG1_CHILD_SETTLED_LAST\n",
    )
    .unwrap();
    std::fs::write(project.path().join("locked.txt"), "NEVER_CHILD_AUTHORIZED").unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    let agent = SubagentAgent {
        id: "worker".into(),
        description: "worker".into(),
        primary: false,
        model: None,
        variant: None,
        prompt: "G1_CHILD_PROFILE".into(),
        permissions: BTreeMap::from([("read".into(), Permission::Ask)]),
        permission_rules: Default::default(),
        hidden: false,
        digest: Some("g1".into()),
        request: Default::default(),
        color: None,
    };
    owner
        .publish_subagents(Some(SubagentCatalog {
            agents: BTreeMap::from([("worker".into(), agent)]),
            depth_limit: 4,
        }))
        .unwrap();
    owner.update_execution(|view| {
        view.generation
            .config
            .permissions
            .insert("subagent".into(), Permission::Allow);
        view.generation.config.permission_rules = crate::permissions::PermissionRules::from_config(
            &json!({"permission":{"subagent":"allow","read":{"*":"allow","locked.txt":"deny"}}}),
        )
        .unwrap();
    });
    let (events, _events) = tokio::sync::broadcast::channel(32);
    owner.set_approval_events(&events);
    owner.register_approval_consumer(false);
    let mut moved = binding_replay_tests::owner(&db, project.path());
    moved.location = "destination".into();
    moved.inherit_application_owners(&owner);
    assert!(!Arc::ptr_eq(&moved.current, &owner.current));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic child".into(),
        timeout: None,
        chunk_timeout_ms: 5000,
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        set_cache_key: false,
        wire: Default::default(),
    };
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([(
            "m".into(),
            json!({"limit":{"context":500000,"output":8000}}),
        )]),
    };
    let (received, held) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = binding_replay_tests::request(&listener).await;
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[
                {"type":"function_call","id":"delegate","call_id":"delegate","name":"subagent","status":"completed",
                 "arguments":json!({"agent":"worker","description":"held child","prompt":"child task only"}).to_string()}
            ]}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        assert!(request["input"].to_string().contains("G1_CHILD_PROFILE"));
        received.send(()).unwrap();
        released.await.unwrap();
        binding_replay_tests::respond(
            &mut socket,
            crate::provider::protocol::Protocol::Responses,
            true,
        )
        .await;
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        let input = request["input"].to_string();
        assert!(
            input.contains("G2_CHILD_PROFILE") && input.contains("G1_CHILD_SETTLED_LAST"),
            "{input}"
        );
        assert!(!input.contains("G1_CHILD_PROFILE"));
        assert!(
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tool| tool["name"] != "shell")
        );
        let calls = [
            json!({"type":"function_call","id":"again","call_id":"again","name":"read","status":"completed","arguments":"{\"path\":\"note.txt\"}"}),
            json!({"type":"function_call","id":"locked","call_id":"locked","name":"read","status":"completed","arguments":"{\"path\":\"locked.txt\"}"}),
        ];
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":calls}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        assert!(request["input"].to_string().contains("denied"));
        assert!(
            !request["input"]
                .to_string()
                .contains("NEVER_CHILD_AUTHORIZED")
        );
        binding_replay_tests::respond(
            &mut socket,
            crate::provider::protocol::Protocol::Responses,
            false,
        )
        .await;
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        assert!(request["input"].to_string().contains("FINAL_PUBLIC"));
        binding_replay_tests::respond(
            &mut socket,
            crate::provider::protocol::Protocol::Responses,
            false,
        )
        .await;
    });
    let cancel = AtomicBool::new(false);
    let turn = owner.run_turn(TurnParams {
        session: "s".into(),
        prompt: "delegate".into(),
        invocation: None,
        catalog: &catalog,
        model_id: "m".into(),
        variant: None,
        max_output: 1000,
        provider,
        cancel: &cancel,
    });
    let publication = async {
        held.await.unwrap();
        owner.update_execution(|view| {
            view.generation.id = 2;
            view.generation.config.tool_output.max_lines = 1;
            view.generation
                .config
                .permissions
                .insert("shell".into(), Permission::Allow);
            view.generation.config.permission_rules =
                crate::permissions::PermissionRules::from_config(
                    &json!({"permission":{"subagent":"allow","read":"allow","shell":"allow"}}),
                )
                .unwrap();
            let agent = view
                .workspace
                .subagents
                .as_mut()
                .unwrap()
                .agents
                .get_mut("worker")
                .unwrap();
            agent.prompt = "G2_CHILD_PROFILE".into();
            agent.digest = Some("g2".into());
            agent.permissions.insert("read".into(), Permission::Allow);
            agent.permissions.insert("shell".into(), Permission::Allow);
        });
        release.send(()).unwrap();
        for generation in [1, 2] {
            let pending = tokio::time::timeout(Duration::from_secs(4), async {
                loop {
                    if let Some(request) = owner.pending_approvals().into_iter().next() {
                        break request;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(pending.binding.generation, generation);
            assert_eq!(
                pending.binding.agent_digest.as_deref(),
                Some(if generation == 1 { "g1" } else { "g2" })
            );
            assert_ne!(pending.binding.session, "s");
            moved
                .reply_approval(ApprovalReply {
                    id: pending.id,
                    binding: pending.binding.clone(),
                    decision: ApprovalDecision::Once,
                })
                .unwrap();
            assert!(
                moved
                    .reply_approval(ApprovalReply {
                        id: pending.id,
                        binding: pending.binding,
                        decision: ApprovalDecision::Once
                    })
                    .is_err()
            );
        }
    };
    let (report, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(turn, publication)
    })
    .await
    .unwrap();
    assert_eq!(report.unwrap().status, TurnStatus::Completed);
    peer.await.unwrap();
    let jobs = db.child_jobs("s").unwrap();
    assert_eq!(jobs.len(), 1);
    let child = &jobs[0];
    let ops = db.list_tool_ops(&child.child.0).unwrap();
    assert_eq!(ops.iter().filter(|op| op.state == "completed").count(), 2);
    let raw = db
        .turn_result(child.turn.as_deref().unwrap())
        .unwrap()
        .1
        .unwrap();
    let log: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(log["requests"][0]["config_generation"], 1);
    assert_eq!(log["requests"][1]["config_generation"], 2);
    owner.child_jobs.shutdown().await.unwrap();
    owner.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn hot01_issued_batch_keeps_output_approval_and_instruction_sources() {
    use oc_core::approval::{ApprovalDecision, ApprovalReply};
    use oc_core::question::{QuestionDecision, QuestionReply};
    use serde_json::json;
    use tokio::io::AsyncWriteExt as _;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("nested")).unwrap();
    std::fs::write(
        project.path().join("nested/note.txt"),
        "G1 first line\nG1 second line\nG1 retained final line\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("nested/AGENTS.md"),
        "G1_NESTED_SOURCE\n",
    )
    .unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    owner.update_execution(|view| {
        view.generation
            .config
            .permissions
            .insert("read".into(), Permission::Ask);
        view.generation
            .config
            .permissions
            .insert("opencode_session_rename".into(), Permission::Allow);
        view.generation.config.permission_rules = crate::permissions::PermissionRules::from_config(
            &json!({"permission":{"read":{"*":"ask","nested/AGENTS.md":"allow"},"opencode_session_rename":"allow","opencode_models":"allow","question":"allow"}}),
        )
        .unwrap();
        view.generation.config.providers.insert("inventory".into(), crate::config::ProviderEntry {
            npm: None, name: None, env: Vec::new(), options: Default::default(),
            models: BTreeMap::from([("G1_METADATA".into(), json!({}))]),
        });
        view.workspace.instruction_roots = vec![crate::instructions::Root {
            path: project.path().into(),
            dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
            origin: crate::instructions::Origin::Project,
            baseline: None,
        }];
    });
    let issued = owner.current.read().unwrap().clone();
    let (events, _events) = tokio::sync::broadcast::channel(16);
    owner.set_approval_events(&events);
    owner.register_approval_consumer(false);
    owner.register_question_consumer();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        timeout: None,
        chunk_timeout_ms: 5000,
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        set_cache_key: false,
        wire: Default::default(),
    };
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([(
            "m".into(),
            json!({"limit":{"context":500000,"output":8000}}),
        )]),
    };
    let (received, held) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = binding_replay_tests::request(&listener).await;
        received.send(()).unwrap();
        released.await.unwrap();
        let calls = [
            json!({"type":"function_call","id":"read","call_id":"read","name":"read","arguments":"{\"path\":\"nested/note.txt\"}","status":"completed"}),
            json!({"type":"function_call","id":"rename","call_id":"rename","name":"opencode_session_rename","arguments":"{\"title\":\"G1 completed batch\"}","status":"completed"}),
            json!({"type":"function_call","id":"models","call_id":"models","name":"opencode_models","arguments":"{\"provider\":\"inventory\"}","status":"completed"}),
            json!({"type":"function_call","id":"question","call_id":"question","name":"question","arguments":json!({"questions":[{"question":"Retain G1?","header":"G1","options":[]}]}).to_string(),"status":"completed"}),
        ];
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":calls}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        let input = request["input"].to_string();
        assert!(
            input.contains("G2_FRESH_PRIMARY"),
            "fresh preparation: {input}"
        );
        assert!(
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tool| tool["name"] != "read"),
            "G2 policy must apply only to the fresh request"
        );
        assert!(
            input.contains("G1 retained final line"),
            "issued output budget, not the new one: {input}"
        );
        assert!(
            input.contains("no longer apply") && !input.contains("G1_NESTED_SOURCE"),
            "fresh roots reconcile the latest instruction state without resurrecting old bodies: {input}"
        );
        assert!(
            input.contains("G1_METADATA") && !input.contains("G2_METADATA"),
            "issued model inventory: {input}"
        );
        assert!(
            input.contains("G1 answer"),
            "captured question result: {input}"
        );
        binding_replay_tests::respond(
            &mut socket,
            crate::provider::protocol::Protocol::Responses,
            false,
        )
        .await;
    });
    let cancel = AtomicBool::new(false);
    let turn = owner.run_turn(TurnParams {
        session: "s".into(),
        prompt: "read then rename".into(),
        invocation: None,
        catalog: &catalog,
        model_id: "m".into(),
        variant: None,
        max_output: 1024,
        provider,
        cancel: &cancel,
    });
    let publication = async {
        held.await.unwrap();
        // Exercise the private publication seam, not a new public busy-reload API.
        owner.update_execution(|view| {
            view.generation.id = issued.id + 1;
            view.generation.config.tool_output.max_lines = 1;
            view.generation
                .config
                .providers
                .get_mut("inventory")
                .unwrap()
                .models = BTreeMap::from([("G2_METADATA".into(), json!({}))]);
            view.workspace.instruction_roots.clear();
            view.workspace.fixed_input = vec![InputItem::message(InputRole::Developer, "G2_FRESH_PRIMARY")];
            view.generation.config.permissions.insert("read".into(), Permission::Deny);
            view.generation.config.permission_rules = crate::permissions::PermissionRules::from_config(
                &json!({"permission":{"read":"deny","opencode_session_rename":"allow","opencode_models":"allow","question":"allow"}}),
            ).unwrap();
        });
        release.send(()).unwrap();
        let request = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if let Some(request) = owner.pending_approvals().into_iter().next() {
                    break request;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(request.binding.generation, issued.id);
        let mut wrong = request.binding.clone();
        wrong.generation = owner.generation_id();
        assert!(
            owner
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: wrong,
                    decision: ApprovalDecision::Once
                })
                .is_err()
        );
        owner
            .reply_approval(ApprovalReply {
                id: request.id,
                binding: request.binding.clone(),
                decision: ApprovalDecision::Once,
            })
            .unwrap();
        assert!(
            owner
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: request.binding,
                    decision: ApprovalDecision::Once
                })
                .is_err()
        );
        let request = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if let Some(request) = owner.pending_questions().into_iter().next() {
                    break request;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(request.binding.generation, issued.id);
        let facts = db.instruction_view("s").unwrap().1;
        assert!(
            facts.iter().any(|fact| fact.source.generation == issued.id
                && fact
                    .source
                    .content
                    .as_deref()
                    .is_some_and(|body| body.contains("G1_NESTED_SOURCE"))),
            "the old read must admit its instructions before the fresh request can remove them"
        );
        let mut reply = QuestionReply {
            id: request.id,
            binding: request.binding,
            decision: QuestionDecision::Answers(vec![vec!["G1 answer".into()]]),
        };
        reply.binding.generation = owner.generation_id();
        assert!(owner.reply_question(reply.clone()).is_err());
        reply.binding.generation = issued.id;
        owner.reply_question(reply.clone()).unwrap();
        assert!(owner.reply_question(reply).is_err());
    };
    let (report, ()) = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(turn, publication)
    })
    .await
    .unwrap();
    let report = report.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls.len(), 4, "{:#?}", report.calls);
    assert!(
        report.calls.iter().all(|call| call.state == "completed"),
        "{:#?}",
        report.calls
    );
    peer.await.unwrap();
    assert_eq!(
        db.session_meta("s").unwrap().title.as_deref(),
        Some("G1 completed batch")
    );
    assert!(
        db.list_tool_ops("s")
            .unwrap()
            .iter()
            .all(|op| op.state == "completed")
    );
    assert_eq!(
        db.instruction_view("s").unwrap().1[0].source.generation,
        issued.id
    );
    owner.shutdown_mcp().await.unwrap();
}

#[test]
fn hot01_issuing_turn_fence_rejects_foreign_settled_and_moved_work() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    db.create_bound_session("other", "work").unwrap();
    db.accept_turn(
        "active",
        "s",
        "task",
        "task",
        &oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        },
    )
    .unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    let mut log = TurnLog::new("active", "m", "fixture");
    log.display = serde_json::json!({"location":"work","move_epoch":0,"owning_operation":"active"});
    assert!(
        owner.check_issued_turn(&log, "s").is_err(),
        "a publication is not an execution lease"
    );
    let lease = owner.begin_active().unwrap();
    owner.update_execution(|view| view.generation.id += 1);
    owner.check_issued_turn(&log, "s").unwrap();
    assert!(owner.check_issued_turn(&log, "other").is_err());
    log.display["move_epoch"] = (-1).into();
    assert!(owner.check_issued_turn(&log, "s").is_err());
    log.display["move_epoch"] = 0.into();
    db.finish_turn("active", "cancelled", None).unwrap();
    assert!(owner.check_issued_turn(&log, "s").is_err());
    drop(lease);
    assert!(db.list_tool_ops("s").unwrap().is_empty());
}

#[tokio::test]
async fn hot01_fresh_provider_capture_changes_route_without_replaying_or_falling_back() {
    use serde_json::json;

    for (retire_model, retire_variant, retire_agent) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("note.txt"),
            "G1 settled first\nG1 settled last\n",
        )
        .unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        let owner = binding_replay_tests::owner(&db, project.path());
        owner.update_execution(|view| view.workspace.agent_id = Some("exact-profile".into()));
        let first = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = ResponsesConfig {
            base_url: format!("http://{}/g1", first.local_addr().unwrap()),
            api_key: "synthetic G1".into(),
            timeout: None,
            chunk_timeout_ms: 3000,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: BTreeMap::new(),
            set_cache_key: false,
            wire: Default::default(),
        };
        let catalog = ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "m".into(),
                json!({
                    "limit":{"context":500000,"output":8000},
                    "variants":{"precise":{"reasoningEffort":"g1-exact"}}
                }),
            )]),
        };
        let mut next_catalog = catalog.clone();
        next_catalog.models.get_mut("m").unwrap()["limit"]["context"] = 100000.into();
        next_catalog.models.get_mut("m").unwrap()["variants"] = if retire_variant {
            json!({})
        } else {
            json!({"precise":{"reasoningEffort":"g2-exact"}})
        };
        if retire_model {
            next_catalog.models.clear();
        }
        let mut next_provider = provider.clone();
        next_provider.base_url = format!("http://{}/g2", second.local_addr().unwrap());
        next_provider.api_key = "synthetic G2".into();
        next_provider.wire.providers.insert(
            "fixture".into(),
            crate::provider::CapturedProvider {
                catalog: next_catalog,
                config: next_provider.clone(),
            },
        );
        let (received, held) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let peer = tokio::spawn(async move {
            let (mut socket, request) = binding_replay_tests::request(&first).await;
            assert_eq!(request["reasoning"]["effort"], "g1-exact");
            received.send(()).unwrap();
            released.await.unwrap();
            binding_replay_tests::respond(
                &mut socket,
                crate::provider::protocol::Protocol::Responses,
                true,
            )
            .await;
            if retire_model || retire_variant || retire_agent {
                assert!(
                    tokio::time::timeout(Duration::from_millis(250), second.accept())
                        .await
                        .is_err(),
                    "removed exact choice must not dispatch a default or sibling"
                );
            } else {
                let (mut socket, request) = tokio::time::timeout(
                    Duration::from_secs(3),
                    binding_replay_tests::request(&second),
                )
                .await
                .unwrap();
                assert_eq!(request["model"], "m");
                assert_eq!(request["reasoning"]["effort"], "g2-exact");
                let input = request["input"].to_string();
                assert!(input.contains("G2_ROUTE_PROMPT") && input.contains("G1 settled last"));
                assert!(
                    input.contains("settled-call") && !input.contains("OPAQUE_CANARY"),
                    "compatible settled pairs survive, foreign opaque does not: {input}"
                );
                assert!(
                    request["tools"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|tool| tool["name"] != "read")
                );
                binding_replay_tests::respond(
                    &mut socket,
                    crate::provider::protocol::Protocol::Responses,
                    false,
                )
                .await;
            }
        });
        let cancel = AtomicBool::new(false);
        let turn = owner.run_turn(TurnParams {
            session: "s".into(),
            prompt: "read once then continue".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: Some("precise".into()),
            max_output: 1000,
            provider,
            cancel: &cancel,
        });
        let publication = async {
            held.await.unwrap();
            owner.update_execution(|view| {
                view.generation.id = 2;
                view.generation
                    .config
                    .permissions
                    .insert("read".into(), Permission::Deny);
                view.generation.config.tool_output.max_lines = 1;
                view.workspace.fixed_input =
                    vec![InputItem::message(InputRole::Developer, "G2_ROUTE_PROMPT")];
                view.request_provider = Some(next_provider);
                if retire_agent {
                    use oc_core::queries::{
                        ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage,
                    };
                    view.workspace.profile_issue = Some(ServiceDiagnostic {
                        kind: ServiceKind::Selection,
                        service: "selection".into(),
                        source: "fixture".into(),
                        field: vec!["selection".into(), "agent".into()],
                        stage: ServiceStage::Admission,
                        code: ServiceCode::AgentUnavailable,
                        action: ServiceAction::SelectAgent,
                    });
                }
            });
            release.send(()).unwrap();
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(8), async {
            tokio::join!(turn, publication)
        })
        .await
        .unwrap();
        if retire_model || retire_variant || retire_agent {
            assert!(
                result.is_err(),
                "an unavailable exact choice must remain unavailable"
            );
            assert_eq!(
                owner.current.read().unwrap().workspace.agent_id.as_deref(),
                Some("exact-profile")
            );
        } else {
            let report = result.unwrap();
            assert_eq!(report.status, TurnStatus::Completed);
            let raw = db.turn_result(&report.turn_id).unwrap().1.unwrap();
            let log: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(log["requests"][0]["config_generation"], 1);
            assert_eq!(log["requests"][1]["config_generation"], 2);
            assert_eq!(log["requests"][1]["context_limit"], 100000);
        }
        assert_eq!(db.list_tool_ops("s").unwrap().len(), 1);
        assert_eq!(db.list_tool_ops("s").unwrap()[0].state, "completed");
        peer.await.unwrap();
        owner.shutdown_mcp().await.unwrap();
    }
}
