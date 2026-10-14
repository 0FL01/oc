use super::*;

const RECONCILE_STDIO: &str = r#"
import json, os, pathlib, sys, time
label = sys.argv[1]
count = int(sys.argv[2]) if len(sys.argv) > 2 else 1
with open('starts', 'a') as output: output.write(label + ':' + str(os.getpid()) + '\n')
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message: continue
    method = message['method']
    if method == 'initialize' and label == 'initializing':
        pathlib.Path('initialize-entered').write_text('ready')
        while not pathlib.Path('initialize-release').exists(): time.sleep(0.002)
    if method == 'initialize': result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{},'resources':{}},'serverInfo':{'name':'reconcile','version':'1'}}
    elif method == 'tools/list': result = {'tools':[{'name':label if count == 1 else label + '_' + str(index),'inputSchema':{'type':'object'}} for index in range(count)]}
    elif method == 'resources/read':
        pathlib.Path('lookup-entered').write_text('ready')
        while not pathlib.Path('lookup-release').exists(): time.sleep(0.002)
        result = {'contents':[{'uri':message['params']['uri'],'mimeType':'text/plain','text':'LOOKUP_ISSUED_RESULT'}]}
    else: result = {'content':[{'type':'text','text':'LEASED_ISSUED_RESULT'}]}
    print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}), flush=True)
"#;

fn reconcile_entry(project: &std::path::Path, label: &str) -> crate::config::McpEntry {
    crate::config::McpEntry {
        kind: "local".into(),
        command: vec![
            "/usr/bin/python3".into(),
            "-u".into(),
            "-c".into(),
            RECONCILE_STDIO.into(),
            label.into(),
        ],
        cwd: Some(project.to_string_lossy().into_owned()),
        resource_admitted: true,
        ..Default::default()
    }
}

async fn connected(owner: &McpOwner, names: &[&str]) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if owner.request_view().is_ok_and(|view| {
                names
                    .iter()
                    .all(|name| view.servers.iter().any(|server| server.server_id == *name))
            }) {
                break;
            }
            assert!(
                owner.fatal().is_none(),
                "fatal while connecting: {:?}",
                owner.fatal()
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn hot01_initial_lane_is_captured_after_the_mcp_request_await() {
    use crate::runtime::tests::binding_replay_tests;
    for retire_model in [false, true] {
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let db = crate::storage::Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        let runtime = binding_replay_tests::owner(&db, project.path());
        runtime.update_execution(|view| {
            view.generation
                .config
                .mcp
                .insert("peer".into(), reconcile_entry(project.path(), "initial"));
            view.workspace.agent_digest = Some("G1_PROFILE".into());
        });
        runtime.start_mcp().unwrap();
        connected(&runtime.mcp_owner(), &["peer"]).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = crate::provider::ResponsesConfig {
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
        let catalog = crate::models::ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "m".into(),
                serde_json::json!({"limit":{"context":500000,"output":8000}}),
            )]),
        };
        let cancel = AtomicBool::new(false);
        let turn = runtime.run_turn(crate::runtime::TurnParams {
            session: "s".into(),
            prompt: "initial coherent preparation".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 1000,
            provider: provider.clone(),
            cancel: &cancel,
        });
        tokio::pin!(turn);
        // This single-threaded fixture has not polled the supervisor's request ACK.
        // No durable acceptance/provider socket is allowed before coherent admission.
        assert!(futures_util::poll!(turn.as_mut()).is_pending());
        assert_eq!(db.history_len("s").unwrap(), 0);
        runtime.update_execution(|view| {
            view.generation.id = 2;
            view.workspace.agent_digest = Some("G2_PROFILE".into());
            view.workspace.fixed_input = vec![crate::provider::InputItem::message(
                crate::provider::InputRole::Developer,
                "G2_INITIAL_LANE",
            )];
            let mut current_catalog = catalog.clone();
            if retire_model {
                current_catalog.models.clear();
            }
            let mut source = provider.clone();
            source.wire.providers.insert(
                "fixture".into(),
                crate::provider::CapturedProvider {
                    catalog: current_catalog,
                    config: provider.clone(),
                },
            );
            view.request_provider = Some(source);
        });
        let peer = async {
            let (mut socket, request) = binding_replay_tests::request(&listener).await;
            assert!(request["input"].to_string().contains("G2_INITIAL_LANE"));
            binding_replay_tests::respond(
                &mut socket,
                crate::provider::protocol::Protocol::Responses,
                false,
            )
            .await;
        };
        let report = if retire_model {
            let result = tokio::time::timeout(Duration::from_secs(5), turn.as_mut())
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
            result
        } else {
            tokio::time::timeout(Duration::from_secs(5), async {
                tokio::join!(turn.as_mut(), peer)
            })
            .await
            .unwrap()
            .0
        };
        runtime.shutdown_mcp().await.unwrap();
        if retire_model {
            assert!(report.is_err());
            assert_eq!(db.history_len("s").unwrap(), 0);
        } else {
            let report = report.unwrap();
            let raw = db.turn_result(&report.turn_id).unwrap().1.unwrap();
            let log: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(log["agent_digest"], "G2_PROFILE");
            assert_eq!(log["display"]["config_generation"], 2);
            assert_eq!(log["requests"][0]["profile"]["agent_digest"], "G2_PROFILE");
        }
        assert!(db.list_tool_ops("s").unwrap().is_empty());
        let pid = std::fs::read_to_string(project.path().join("starts")).unwrap();
        let pid = pid.trim().split_once(':').unwrap().1;
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[tokio::test]
async fn hot02_reconcile_reuses_clients_and_coalesces_replacement_until_old_batch_releases() {
    let project = tempfile::tempdir().unwrap();
    let owner = McpOwner::new("source", 1);
    let mut config = Generation {
        mcp: BTreeMap::from([
            ("peer".into(), reconcile_entry(project.path(), "old")),
            ("other".into(), reconcile_entry(project.path(), "unrelated")),
        ]),
        ..Default::default()
    };
    owner
        .start(config.clone(), project.path().into(), BTreeMap::new(), None)
        .unwrap();
    connected(&owner, &["peer", "other"]).await;
    let initial_binding = owner.snapshot().binding;
    let held = owner.request_view().unwrap();
    let original = &held
        .servers
        .iter()
        .find(|server| server.server_id == "peer")
        .unwrap()
        .client;
    let unrelated = Arc::downgrade(
        &held
            .servers
            .iter()
            .find(|server| server.server_id == "other")
            .unwrap()
            .client,
    );
    let late_unknown = held.remote_unknown.clone();

    // A Code Mode edit changes only the new immutable view, not either client.
    config.mcp.get_mut("peer").unwrap().codemode = Some(false);
    owner
        .reconcile(config.clone(), 2, None, false)
        .await
        .unwrap();
    let routing = owner.request_view().unwrap();
    assert!(held.code_mode_server("peer"));
    assert!(!routing.code_mode_server("peer"));
    assert!(Arc::ptr_eq(
        original,
        &routing
            .servers
            .iter()
            .find(|server| server.server_id == "peer")
            .unwrap()
            .client
    ));
    assert_eq!(owner.snapshot().binding.instance, initial_binding.instance);

    for (publication, label) in [(3, "discarded"), (4, "latest")] {
        config
            .mcp
            .insert("peer".into(), reconcile_entry(project.path(), label));
        owner
            .reconcile(config.clone(), publication, None, false)
            .await
            .unwrap();
        let next = owner.request_view().unwrap();
        assert_eq!(next.servers.len(), 1);
        assert_eq!(next.servers[0].server_id, "other");
        assert!(owner.snapshot().servers.iter().any(|row| row.name == "peer"
            && row.status == McpStatus::Pending
            && row.pending_action == Some(McpAction::Disconnect)));
    }
    // The issued old request can still execute after removal from new catalogs.
    let AttachedServer::Stdio(client) = original.as_ref() else {
        panic!("stdio fixture")
    };
    assert!(
        client
            .call_tool("old", serde_json::json!({}), &AtomicBool::new(false))
            .await
            .unwrap()
            .contains("LEASED_ISSUED_RESULT")
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let old_pid = std::fs::read_to_string(project.path().join("starts"))
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("old:").map(str::to_owned))
        .unwrap();
    assert!(std::path::Path::new(&format!("/proc/{old_pid}")).exists());
    drop(routing);
    drop(held);
    connected(&owner, &["peer", "other"]).await;
    let current = owner.request_view().unwrap();
    assert!(current.entries.iter().any(|entry| entry.tool == "latest"));
    assert!(
        !current
            .entries
            .iter()
            .any(|entry| entry.tool == "old" || entry.tool == "discarded")
    );
    assert!(std::sync::Weak::ptr_eq(
        &unrelated,
        &Arc::downgrade(
            &current
                .servers
                .iter()
                .find(|server| server.server_id == "other")
                .unwrap()
                .client
        )
    ));
    assert!(!std::path::Path::new(&format!("/proc/{old_pid}")).exists());
    let starts = std::fs::read_to_string(project.path().join("starts")).unwrap();
    assert_eq!(starts.lines().count(), 3);
    assert!(!starts.lines().any(|line| line.starts_with("discarded:")));

    let (ack, reply) = oneshot::channel();
    owner.control(
        McpControl {
            binding: initial_binding,
            server: owner.snapshot().servers[0].id.clone(),
            action: McpAction::Disconnect,
        },
        ack,
    );
    assert!(
        reply.await.unwrap().is_err(),
        "old configuration control affected the new binding"
    );
    assert!(Arc::ptr_eq(&late_unknown, &current.remote_unknown));
    drop(current);
    let snapshot = owner.snapshot();
    let other = snapshot
        .servers
        .iter()
        .find(|row| row.name == "other")
        .unwrap()
        .id
        .clone();
    let (ack, reply) = oneshot::channel();
    owner.control(
        McpControl {
            binding: snapshot.binding,
            server: other.clone(),
            action: McpAction::Disconnect,
        },
        ack,
    );
    reply.await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !owner
            .snapshot()
            .servers
            .iter()
            .any(|row| row.id == other && row.status == McpStatus::Disabled)
        {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    owner.reconcile(config, 5, None, false).await.unwrap();
    assert!(
        !owner
            .request_view()
            .unwrap()
            .servers
            .iter()
            .any(|server| server.server_id == "other"),
        "automatic no-op undid manual disconnect"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    // This is the exact flag a late old-call lease sets; no physical remote
    // request is issued by this stdio fixture. Empty publication cannot reset it.
    owner
        .reconcile(Generation::default(), 6, None, false)
        .await
        .unwrap();
    late_unknown.store(true, Ordering::SeqCst);
    assert!(matches!(
        owner.request_view(),
        Err(RuntimeError::McpAttach {
            safe_code: "unsafe_retry",
            ..
        })
    ));
    owner.stop().await.unwrap();
    for line in starts.lines() {
        let (_, pid) = line.rsplit_once(':').unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[tokio::test]
async fn hot02_reconcile_bounds_connecting_live_and_retiring_across_repeated_edits() {
    let project = tempfile::tempdir().unwrap();
    let owner = McpOwner::new("source", 1);
    let configured = |prefix: &str| Generation {
        mcp: (0..MAX_MCP_SERVERS)
            .map(|index| {
                let name = format!("{prefix}_{index}");
                (name.clone(), reconcile_entry(project.path(), &name))
            })
            .collect(),
        ..Default::default()
    };
    owner
        .start(
            configured("old"),
            project.path().into(),
            BTreeMap::new(),
            None,
        )
        .unwrap();
    let old_names: Vec<_> = (0..MAX_MCP_SERVERS)
        .map(|index| format!("old_{index}"))
        .collect();
    connected(
        &owner,
        &old_names.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await;
    let held = owner.request_view().unwrap();
    let before = owner.snapshot();
    let mut oversized = configured("rejected");
    oversized.mcp.insert(
        "one_too_many".into(),
        reconcile_entry(project.path(), "forbidden"),
    );
    assert!(owner.reconcile(oversized, 2, None, false).await.is_err());
    assert_eq!(
        owner.snapshot(),
        before,
        "invalid desired inventory retired good clients"
    );
    for publication in 2..14 {
        owner
            .reconcile(
                configured(&format!("edit{publication}")),
                publication,
                None,
                false,
            )
            .await
            .unwrap();
        assert!(owner.request_view().unwrap().servers.is_empty());
        assert!(
            owner.snapshot().servers.len() <= MAX_MCP_SERVERS * 2,
            "obsolete desired rows accumulated"
        );
        assert_eq!(
            std::fs::read_to_string(project.path().join("starts"))
                .unwrap()
                .lines()
                .count(),
            MAX_MCP_SERVERS
        );
    }
    drop(held);
    let names: Vec<_> = (0..MAX_MCP_SERVERS)
        .map(|index| format!("edit13_{index}"))
        .collect();
    connected(
        &owner,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await;
    let starts = std::fs::read_to_string(project.path().join("starts")).unwrap();
    assert_eq!(starts.lines().count(), MAX_MCP_SERVERS * 2);
    assert_eq!(owner.snapshot().servers.len(), MAX_MCP_SERVERS);
    assert!(
        starts
            .lines()
            .all(|line| line.starts_with("old_") || line.starts_with("edit13_"))
    );
    owner.stop().await.unwrap();
    for line in starts.lines() {
        let (_, pid) = line.rsplit_once(':').unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[tokio::test]
async fn hot02_reload_keeps_the_location_supervisor_instance() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let runtime = crate::runtime::tests::binding_replay_tests::owner(&db, project.path());
    runtime.start_mcp().unwrap();
    let owner = runtime.mcp_owner();
    let binding = owner.snapshot().binding;
    let id = runtime.reload(Generation::default()).await.unwrap();
    let replacement = runtime.mcp_owner();
    let next = replacement.snapshot().binding;
    // Simulate a caller disappearing after the supervisor accepted a desired
    // publication, before the execution publication was committed. Counters
    // alone neither grant nor deny the next request's MCP authority.
    owner
        .reconcile(Generation::default(), id + 1, None, false)
        .await
        .unwrap();
    assert!(runtime.request_mcp(&AtomicBool::new(false)).await.is_ok());
    owner
        .reconcile(
            Generation {
                mcp: BTreeMap::from([(
                    "uncommitted".into(),
                    crate::config::McpEntry {
                        enabled: false,
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
            id + 1,
            None,
            false,
        )
        .await
        .unwrap();
    assert!(matches!(
        runtime.request_mcp(&AtomicBool::new(false)).await,
        Err(RuntimeError::InvalidArgs(_))
    ));
    runtime.reload(Generation::default()).await.unwrap();
    runtime.quarantine_remote_retries();
    runtime
        .reload(Generation {
            mcp: BTreeMap::from([(
                "blocked".into(),
                crate::config::McpEntry {
                    kind: "remote".into(),
                    url: Some("https://must-not-resolve.invalid/mcp".into()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
        .await
        .unwrap();
    let quarantined = runtime.mcp_status();
    runtime.shutdown_mcp().await.unwrap();
    assert!(
        Arc::ptr_eq(&owner, &replacement),
        "reload replaced the Location supervisor"
    );
    assert_eq!(binding.instance, next.instance);
    assert_eq!(next.generation, id);
    assert_eq!(quarantined.servers[0].status, McpStatus::Failed);
    assert_eq!(
        quarantined.servers[0].diagnostic.as_ref().unwrap().code,
        ServiceCode::UnsafeRetry
    );
    assert!(db.list_sessions().unwrap().is_empty());
}

#[tokio::test]
async fn hot02_pending_connection_adopts_latest_routing_without_reconnecting() {
    let project = tempfile::tempdir().unwrap();
    let owner = McpOwner::new("source", 1);
    let mut config = Generation {
        mcp: BTreeMap::from([(
            "peer".into(),
            reconcile_entry(project.path(), "initializing"),
        )]),
        ..Default::default()
    };
    owner
        .start(config.clone(), project.path().into(), BTreeMap::new(), None)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !project.path().join("initialize-entered").exists() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    config.mcp.get_mut("peer").unwrap().codemode = Some(false);
    owner.reconcile(config, 2, None, false).await.unwrap();
    std::fs::write(project.path().join("initialize-release"), "release").unwrap();
    connected(&owner, &["peer"]).await;
    let view = owner.request_view().unwrap();
    assert_eq!(view.publication, 2);
    assert!(
        !view.code_mode_server("peer"),
        "late initialize restored stale view metadata"
    );
    drop(view);
    owner.stop().await.unwrap();
    let starts = std::fs::read_to_string(project.path().join("starts")).unwrap();
    assert_eq!(starts.lines().count(), 1);
    let (_, pid) = starts.trim().rsplit_once(':').unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

#[tokio::test]
async fn hot02_reconcile_preserves_issued_lookup_and_joins_removed_connection() {
    use oc_core::domain::SessionId;
    use oc_core::queries::{McpLookup, McpLookupOp};
    let project = tempfile::tempdir().unwrap();
    let owner = McpOwner::new("source", 1);
    owner
        .start(
            Generation {
                mcp: BTreeMap::from([("peer".into(), reconcile_entry(project.path(), "held"))]),
                ..Default::default()
            },
            project.path().into(),
            BTreeMap::new(),
            None,
        )
        .unwrap();
    connected(&owner, &["peer"]).await;
    let snapshot = owner.snapshot();
    let cancel = Arc::new(AtomicBool::new(false));
    let (ack, reply) = oneshot::channel();
    owner.enqueue_lookup(
        McpLookup {
            binding: snapshot.binding.clone(),
            server: snapshot.servers[0].id.clone(),
            session: SessionId("lookup".into()),
            operation: McpLookupOp::ReadResource {
                uri: "fixture://retained".into(),
            },
            refresh: false,
        },
        cancel.clone(),
        ack,
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !project.path().join("lookup-entered").exists() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    owner
        .reconcile(Generation::default(), 2, None, false)
        .await
        .unwrap();
    assert!(
        !cancel.load(Ordering::SeqCst),
        "automatic retirement canceled issued lookup"
    );
    assert!(owner.request_view().unwrap().servers.is_empty());
    assert_eq!(
        owner.snapshot().servers[0].pending_action,
        Some(McpAction::Disconnect)
    );
    std::fs::write(project.path().join("lookup-release"), "release").unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(5), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        reply.binding, snapshot.binding,
        "old operation was rebound to latest"
    );
    assert_eq!(
        serde_json::to_value(&reply.data).unwrap()["contents"][0]["text"],
        "LOOKUP_ISSUED_RESULT"
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !owner.snapshot().servers.is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    owner.stop().await.unwrap();
    let starts = std::fs::read_to_string(project.path().join("starts")).unwrap();
    let (_, pid) = starts.trim().rsplit_once(':').unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

#[tokio::test]
async fn hot02_prospective_aggregate_catalog_limit_is_fatal_before_exposure() {
    let project = tempfile::tempdir().unwrap();
    let owner = McpOwner::new("source", 1);
    let entry = |name| {
        let mut entry = reconcile_entry(project.path(), name);
        entry.command.push("50".into());
        entry
    };
    owner
        .start(
            Generation {
                mcp: BTreeMap::from([
                    ("one".into(), entry("first")),
                    ("two".into(), entry("second")),
                    ("three".into(), entry("third")),
                ]),
                ..Default::default()
            },
            project.path().into(),
            BTreeMap::new(),
            None,
        )
        .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), owner.wait_failure())
        .await
        .unwrap();
    assert_eq!(error, RuntimeError::McpCatalogLimit);
    assert!(
        owner
            .shared
            .publication
            .read()
            .unwrap()
            .request
            .entries
            .len()
            <= mcp_remote::GENERATION_TOOLS_CAP
    );
    assert!(matches!(
        owner.request_view(),
        Err(RuntimeError::McpCatalogLimit)
    ));
    assert_eq!(owner.stop().await, Err(RuntimeError::McpCatalogLimit));
    let starts = std::fs::read_to_string(project.path().join("starts")).unwrap();
    assert_eq!(starts.lines().count(), 3);
    for line in starts.lines() {
        let (_, pid) = line.rsplit_once(':').unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}

fn pending_scope(owner: &McpOwner) -> Scope {
    Scope {
        shared: owner.shared.clone(),
        config: Generation::default(),
        project: std::path::PathBuf::from("fixture"),
        env: BTreeMap::new(),
        activation: None,
        nodes: BTreeMap::from([(
            "peer".into(),
            Node {
                incarnation: Arc::new(()),
                row: McpServerSnapshot {
                    id: "mcp-peer".into(),
                    name: "peer".into(),
                    configured_enabled: true,
                    status: McpStatus::Pending,
                    pending_action: Some(McpAction::Connect),
                    tools: 0,
                    diagnostic: None,
                    actions: Vec::new(),
                },
                source: "native fixture".into(),
                desired: true,
                cancel: Some(Arc::new(AtomicBool::new(false))),
                closing: false,
                replacing: false,
                catalogs: [None, None, None],
                catalog_pending: [false; 3],
            },
        )]),
        generation: McpGeneration::empty(2, owner.shared.wake.clone()),
        work: JoinSet::new(),
        retiring: Vec::new(),
        instances: Vec::new(),
        requests: Vec::new(),
        lookups: Vec::new(),
        fatal: None,
        stopping: false,
    }
}

#[test]
fn hot02_retired_completions_cannot_mutate_replacement_or_hide_cleanup_failure() {
    let owner = McpOwner::new("fixture", 2);
    let mut scope = pending_scope(&owner);
    let current = scope.nodes["peer"].incarnation.clone();
    let cancel = scope.nodes["peer"].cancel.as_ref().unwrap().clone();
    let before = scope.nodes["peer"].row.clone();
    let retired = Arc::new(());
    for result in [
        Ok(WorkResult::Refreshed(Vec::new())),
        Ok(WorkResult::Closed),
        Err(RuntimeError::McpAttach {
            server: "peer".into(),
            stage: "tools-list",
            safe_code: "transport",
            retryable: true,
        }),
    ] {
        scope.complete(Completed {
            server: "peer".into(),
            incarnation: retired.clone(),
            refresh: false,
            result,
        });
        assert_eq!(scope.nodes["peer"].row, before);
        assert!(
            scope.nodes["peer"]
                .cancel
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &cancel)),
            "retired completion consumed the replacement's cancellation/ownership"
        );
        assert!(Arc::ptr_eq(&scope.nodes["peer"].incarnation, &current));
        assert!(!cancel.load(Ordering::SeqCst));
        assert!(scope.generation.degraded.is_empty());
    }
    // Cleanup is not optional just because its connection is no longer current.
    scope.complete(Completed {
        server: "peer".into(),
        incarnation: retired,
        refresh: false,
        result: Err(RuntimeError::McpShutdown),
    });
    assert_eq!(scope.fatal, Some(RuntimeError::McpShutdown));
    assert!(scope.stopping);
    assert!(matches!(
        owner.request_view(),
        Err(RuntimeError::McpShutdown)
    ));
}

#[tokio::test]
async fn hot02_late_connected_client_is_hidden_leased_then_reaped_without_touching_replacement() {
    const STDIO: &str = r#"
import json, os, pathlib, sys
pathlib.Path('mcp.pid').write_text(str(os.getpid()))
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message: continue
    result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{}},'serverInfo':{'name':'late','version':'1'}} if message['method'] == 'initialize' else {'tools':[{'name':'old','inputSchema':{'type':'object'}}]}
    print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}), flush=True)
"#;
    let project = tempfile::tempdir().unwrap();
    let entry = crate::config::McpEntry {
        kind: "local".into(),
        command: vec![
            "/usr/bin/python3".into(),
            "-u".into(),
            "-c".into(),
            STDIO.into(),
        ],
        cwd: Some(project.path().to_string_lossy().into_owned()),
        resource_admitted: true,
        ..Default::default()
    };
    let old = connect_one(
        "peer",
        &entry,
        project.path(),
        &BTreeMap::new(),
        &[],
        &AtomicBool::new(false),
    )
    .await
    .unwrap();
    let pid: u32 = std::fs::read_to_string(project.path().join("mcp.pid"))
        .unwrap()
        .parse()
        .unwrap();
    let owner = McpOwner::new("fixture", 2);
    let mut scope = pending_scope(&owner);
    let current = scope.nodes["peer"].incarnation.clone();
    let before = scope.nodes["peer"].row.clone();
    let mut held = McpGeneration::empty(1, owner.shared.wake.clone());
    held.servers.push(old.clone());
    scope.complete(Completed {
        server: "peer".into(),
        incarnation: Arc::new(()),
        refresh: false,
        result: Ok(WorkResult::Connected(old)),
    });
    let hidden = scope.generation.servers.is_empty() && scope.generation.entries.is_empty();
    let leased = scope.retiring.len() == 1 && scope.work.is_empty();
    drop(held);
    scope.drain_retiring();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(result) = scope.work.join_next().await {
            scope.complete(result.unwrap());
        }
    })
    .await
    .unwrap();
    let unchanged = scope.nodes["peer"].row == before
        && Arc::ptr_eq(&scope.nodes["peer"].incarnation, &current)
        && scope.nodes["peer"].cancel.is_some();
    scope.begin_stop();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(result) = scope.work.join_next().await {
            scope.complete(result.unwrap());
        }
    })
    .await
    .unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert!(scope.retiring.is_empty());
    assert!(hidden, "late success exposed the retired client's registry");
    assert!(
        leased,
        "late success bypassed its existing physical/batch lease"
    );
    assert!(unchanged, "old cleanup rewrote the replacement incarnation");
}

#[test]
fn completion_warning_union_is_typed_and_exact_binding_scoped() {
    let owner = Arc::new(McpOwner::new("old-location", 7));
    let binding = owner.snapshot().binding;
    let original = stdio_attach_error_at("unavailable", "initialize", StdioError::Transport);
    let changed = stdio_attach_error_at("unavailable", "tools-list", StdioError::Deadline);
    let mut initial = McpGeneration::empty(7, owner.shared.wake.clone());
    initial.degraded = vec![original.clone()];
    let mut latest = McpGeneration::empty(7, owner.shared.wake.clone());
    latest.degraded = vec![original.clone(), changed.clone()];
    owner.shared.publication.write().unwrap().request = Arc::new(latest);
    assert_eq!(
        owner.completion_warnings(&binding, &initial.degraded),
        [original.to_string(), changed.to_string()]
    );
    assert_eq!(
        initial.degraded.as_slice(),
        std::slice::from_ref(&original),
        "completion mutated the held lease"
    );
    for mismatch in [
        McpBinding {
            location: "new-location".into(),
            ..binding.clone()
        },
        McpBinding {
            generation: 8,
            ..binding.clone()
        },
        McpBinding {
            instance: binding.instance + 1,
            ..binding.clone()
        },
    ] {
        assert_eq!(
            owner.completion_warnings(&mismatch, &initial.degraded),
            [original.to_string()]
        );
    }
    let replacement = Arc::new(McpOwner::new("old-location", 7));
    let mut foreign = McpGeneration::empty(7, replacement.shared.wake.clone());
    foreign.degraded = vec![changed];
    replacement.shared.publication.write().unwrap().request = Arc::new(foreign);
    assert_eq!(
        replacement.completion_warnings(&binding, &initial.degraded),
        [original.to_string()],
        "same Location/generation but different owner instance mixed warnings"
    );
    // Diagnostic observation cannot clear or demote an existing fatal result.
    owner.shared.publication.write().unwrap().fatal = Some(RuntimeError::McpShutdown);
    let _ = owner.completion_warnings(&binding, &initial.degraded);
    assert!(matches!(
        owner.request_view(),
        Err(RuntimeError::McpShutdown)
    ));
}

#[test]
fn successful_retry_clears_the_exact_sanitized_failure_identity() {
    assert!(!safe_server_id("known-peer.example").contains("known-peer"));
    for server in ["peer_日本語", &"long_peer_".repeat(12)] {
        let mut degraded = Vec::new();
        record_degradation(
            &mut degraded,
            server,
            stdio_attach_error_at(server, "initialize", StdioError::Spawn),
        );
        assert_eq!(degraded.len(), 1);
        assert!(degraded[0].to_string().len() < 160);
        clear_degradation(&mut degraded, server);
        assert!(degraded.is_empty(), "healthy retry retained stale warning");
    }
    let prefix = "long_peer_".repeat(12);
    for (one, two) in [
        (format!("{prefix}one"), format!("{prefix}two")),
        ("peer_日本語".into(), "peer_中文語".into()),
    ] {
        let mut degraded = Vec::new();
        for server in [&one, &two] {
            record_degradation(
                &mut degraded,
                server,
                stdio_attach_error_at(server, "initialize", StdioError::Spawn),
            );
            assert!(safe_server_id(server).len() <= 64);
        }
        assert_eq!(degraded.len(), 2, "one failed neighbor hid another");
        clear_degradation(&mut degraded, &one);
        assert_eq!(degraded.len(), 1);
        assert!(
            matches!(&degraded[0], RuntimeError::McpAttach { server, .. } if server == &safe_server_id(&two))
        );
        clear_degradation(&mut degraded, &two);
        assert!(degraded.is_empty());
    }
}

#[tokio::test]
async fn failed_supervisor_join_remains_fatal_after_its_handle_is_consumed() {
    for panic in [false, true] {
        let owner = McpOwner::new("fixture", 1);
        *owner.task.lock().await = Some(tokio::spawn(async move {
            assert!(!panic, "fixture supervisor interruption");
            Err(RuntimeError::McpShutdown)
        }));
        assert_eq!(owner.stop().await, Err(RuntimeError::McpShutdown));
        assert_eq!(owner.stop().await, Err(RuntimeError::McpShutdown));
        assert!(owner.cleanup_failed());
        assert!(matches!(
            owner.request_view(),
            Err(RuntimeError::McpShutdown)
        ));
    }
}

#[test]
fn configured_display_label_keeps_unicode_and_long_names_but_not_sensitive_channels() {
    for label in [
        "chrome-devtools",
        "codex_web",
        "crw",
        "日本語のMCP",
        &"long_name_".repeat(100),
    ] {
        assert!(safe_display_label(label, &[]));
    }
    for label in [
        "",
        "https://private.invalid/mcp",
        "/private/credential",
        "../credential",
        "~/credential",
        "C:\\private\\credential",
        "mcp\u{1b}[31m",
        "mcp\nname",
        "mcp\u{202e}name",
        "{file:credential}",
        "{env:AUTH}",
    ] {
        assert!(!safe_display_label(label, &[]));
    }
    assert!(!safe_display_label(
        "prefix-protected-suffix",
        &["protected".into()]
    ));
    assert!(safe_display_label("ordinary", &[String::new()]));
}
