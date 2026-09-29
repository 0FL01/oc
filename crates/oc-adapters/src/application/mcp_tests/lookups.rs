use super::*;
use oc_core::queries::{
    McpLookup, McpLookupData, McpLookupError, McpLookupOp, SessionSelectionAction,
};

fn entry(f: &Fixture, mode: &str, report: &Path, gate: &Path) -> Value {
    json!({"type":"local", "command":["/usr/bin/python3",
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mcp11-lookups.py"),mode,report,gate],
        "environment":{"LOOKUP_CANARY":"LOOKUP_SECRET_CANARY"},
        "timeout":{"startup":3000,"catalog":3000,"execution":3000}, "cwd":f.project})
}
fn setup(f: &Fixture, mcp: Value, permission: Value, agents: Value) {
    f.config(mcp);
    let path = f.global.join("opencode.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["permission"] = permission;
    config["agent"] = agents;
    fs::write(path, config.to_string()).unwrap();
}
fn query(snapshot: &McpSnapshot, server: &str, session: &str, operation: McpLookupOp) -> McpLookup {
    McpLookup {
        binding: snapshot.binding.clone(),
        server: snapshot
            .servers
            .iter()
            .find(|r| r.name == crate::config::mcp::safe_identity(server))
            .unwrap()
            .id
            .clone(),
        session: SessionId(session.into()),
        operation,
        refresh: true,
    }
}
fn count(report: &Path, method: &str) -> usize {
    fs::read_to_string(report)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["method"] == method)
        .count()
}
async fn wait_count(report: &Path, method: &str, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while count(report, method) < expected {
        assert!(
            Instant::now() < deadline,
            "bounded fixture counter {method}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn pid(report: &Path) -> libc::pid_t {
    fs::read_to_string(report)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .rfind(|event| event["method"] == "spawn")
        .unwrap()["pid"]
        .as_i64()
        .unwrap() as libc::pid_t
}
fn gone(pid: libc::pid_t) {
    // SAFETY: signal zero probes only an owned PID recorded by this fixture.
    assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
}

#[tokio::test]
async fn mcp11_core_caller_retains_healthy_catalog_and_keeps_lookup_out_of_history_and_tools() {
    let f = Fixture::new();
    let report = f.project.join("lookups.events");
    setup(
        &f,
        json!({"peer":entry(&f,"basic",&report,&f.project.join("gate"))}),
        json!({"mcp_lookup":"allow","read":"allow"}),
        json!({}),
    );
    let bytes = fs::read(f.global.join("opencode.json")).unwrap();
    let (app, guard) = f.spawn().await;
    app.create_session(SessionId("lookup-root".into()))
        .await
        .unwrap();
    let (snapshot, row) = wait_status(&app, "peer", McpStatus::Connected).await;
    let q = query(&snapshot, "peer", "lookup-root", McpLookupOp::ListPrompts);
    let initial = app.mcp_lookup(q.clone()).await.unwrap();
    assert_eq!(initial.data.catalog_entries(), 2);
    assert!(!initial.cached);
    assert!(initial.source.starts_with("source-"));
    for operation in [
        McpLookupOp::ListResources,
        McpLookupOp::ListResourceTemplates,
    ] {
        assert_eq!(
            app.mcp_lookup(query(&snapshot, "peer", "lookup-root", operation))
                .await
                .unwrap()
                .data
                .catalog_entries(),
            1
        );
    }
    assert_eq!(count(&report, "resources/read"), 0);
    assert_eq!(count(&report, "prompts/get"), 0);
    let prompt = app
        .mcp_lookup(query(
            &snapshot,
            "peer",
            "lookup-root",
            McpLookupOp::GetPrompt {
                name: "outline".into(),
                arguments: BTreeMap::from([("topic".into(), "synthetic".into())]),
            },
        ))
        .await
        .unwrap();
    assert!(matches!(prompt.data, McpLookupData::Prompt { messages, .. } if messages.len() == 2));
    let resource = app
        .mcp_lookup(query(
            &snapshot,
            "peer",
            "lookup-root",
            McpLookupOp::ReadResource {
                uri: "fixture://text".into(),
            },
        ))
        .await
        .unwrap();
    assert!(
        !serde_json::to_string(&resource.data)
            .unwrap()
            .contains("LOOKUP_SECRET_CANARY")
    );
    assert!(!format!("{resource:?}").contains("Explicit resource"));
    fs::write(report.with_extension("events.fail"), "failure").unwrap();
    assert_eq!(
        app.mcp_lookup(q.clone()).await,
        Err(McpLookupError::RemoteFailure)
    );
    let mut cached = q;
    cached.refresh = false;
    let retained = app.mcp_lookup(cached).await.unwrap();
    assert!(retained.cached);
    assert_eq!(retained.data, initial.data);
    assert_eq!(app.mcp_status().await.unwrap().servers[0].tools, row.tools);
    assert_eq!(count(&report, "initialize"), 1);
    assert_eq!(count(&report, "tools/list"), 1);
    assert_eq!(count(&report, "tools/call"), 0);
    assert_eq!(fs::read(f.global.join("opencode.json")).unwrap(), bytes);
    let child = pid(&report);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    gone(child);
    let db = Db::open(&f.data).unwrap();
    assert_eq!(db.history_len("lookup-root").unwrap(), 0);
    assert_eq!(db.tool_ops_len("lookup-root").unwrap(), 0);
}

#[tokio::test]
async fn mcp11_core_session_agent_deny_and_ask_are_enforced_before_body_rpc() {
    let f = Fixture::new();
    let report = f.project.join("policy.events");
    setup(
        &f,
        json!({"peer":entry(&f,"basic",&report,&f.project.join("gate"))}),
        json!({"mcp_lookup":"allow","read":"allow"}),
        json!({"strict":{"mode":"primary","permission":{"read":"deny"}}}),
    );
    let (app, guard) = f.spawn().await;
    app.create_session(SessionId("strict-root".into()))
        .await
        .unwrap();
    app.create_session(SessionId("normal-root".into()))
        .await
        .unwrap();
    app.session_selection(
        SessionId("normal-root".into()),
        false,
        SessionSelectionAction::Current,
    )
    .await
    .unwrap();
    app.session_selection(
        SessionId("strict-root".into()),
        false,
        SessionSelectionAction::Agent("strict".into()),
    )
    .await
    .unwrap();
    let (snapshot, _) = wait_status(&app, "peer", McpStatus::Connected).await;
    let read = McpLookupOp::ReadResource {
        uri: "fixture://text".into(),
    };
    assert_eq!(
        app.mcp_lookup(query(&snapshot, "peer", "strict-root", read.clone()))
            .await,
        Err(McpLookupError::PermissionDenied)
    );
    assert_eq!(
        app.mcp_lookup(query(&snapshot, "peer", "missing-root", read.clone()))
            .await,
        Err(McpLookupError::SessionMismatch)
    );
    assert_eq!(count(&report, "resources/read"), 0);
    app.mcp_lookup(query(&snapshot, "peer", "normal-root", read))
        .await
        .unwrap();
    assert_eq!(count(&report, "resources/read"), 1);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let f = Fixture::new();
    let report = f.project.join("ask.events");
    setup(
        &f,
        json!({"peer":entry(&f,"basic",&report,&f.project.join("gate"))}),
        json!({"mcp_lookup":"ask","read":"allow"}),
        json!({}),
    );
    let (app, guard) = f.spawn().await;
    app.create_session(SessionId("ask-root".into()))
        .await
        .unwrap();
    app.register_approval_consumer(true).await.unwrap();
    let (snapshot, _) = wait_status(&app, "peer", McpStatus::Connected).await;
    assert_eq!(
        app.mcp_lookup(query(
            &snapshot,
            "peer",
            "ask-root",
            McpLookupOp::ListPrompts
        ))
        .await,
        Err(McpLookupError::ApprovalRequired)
    );
    assert_eq!(count(&report, "prompts/list"), 0);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn mcp11_core_drop_disconnect_reload_location_cancel_only_owned_lookup_and_reap() {
    let f = Fixture::new();
    let report = f.project.join("held.events");
    let sibling = f.project.join("sibling.events");
    let gate = f.project.join("held-gate");
    setup(
        &f,
        json!({}),
        json!({"mcp_lookup":"allow","read":"allow"}),
        json!({}),
    );
    fs::write(
        f.project.join("opencode.json"),
        json!({"mcp":{"peer":entry(&f,"held",&report,&gate),
        "sibling":entry(&f,"basic",&sibling,&gate)}})
        .to_string(),
    )
    .unwrap();
    let (app, guard) = f.spawn().await;
    app.create_session(SessionId("held-root".into()))
        .await
        .unwrap();
    let (snapshot, _) = wait_status(&app, "peer", McpStatus::Connected).await;
    wait_status(&app, "sibling", McpStatus::Connected).await;
    let q = query(
        &snapshot,
        "peer",
        "held-root",
        McpLookupOp::ReadResource {
            uri: "fixture://held".into(),
        },
    );
    let task = tokio::spawn({
        let app = app.clone();
        let q = q.clone();
        async move { app.mcp_lookup(q).await }
    });
    wait_count(&report, "resources/read", 1).await;
    let responsive = tokio::time::timeout(Duration::from_millis(500), app.mcp_status())
        .await
        .unwrap()
        .unwrap();
    app.mcp_control(control(&responsive, "sibling", McpAction::Disconnect))
        .await
        .unwrap();
    wait_status(&app, "sibling", McpStatus::Disabled).await;
    gone(pid(&sibling));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    wait_count(&report, "cancel_observed", 1).await;
    app.mcp_lookup(query(
        &snapshot,
        "peer",
        "held-root",
        McpLookupOp::ListPrompts,
    ))
    .await
    .unwrap();
    let old_pid = pid(&report);
    let pending = tokio::spawn({
        let app = app.clone();
        let q = q.clone();
        async move { app.mcp_lookup(q).await }
    });
    wait_count(&report, "resources/read", 2).await;
    app.reload_location().await.unwrap();
    assert_eq!(pending.await.unwrap(), Err(McpLookupError::Cancelled));
    gone(old_pid);
    let (new, _) = wait_status(&app, "peer", McpStatus::Connected).await;
    assert_ne!(new.binding, snapshot.binding);
    assert_eq!(app.mcp_lookup(q).await, Err(McpLookupError::StaleBinding));
    let pending = tokio::spawn({
        let app = app.clone();
        let q = query(
            &new,
            "peer",
            "held-root",
            McpLookupOp::ReadResource {
                uri: "fixture://held".into(),
            },
        );
        async move { app.mcp_lookup(q).await }
    });
    wait_count(&report, "resources/read", 3).await;
    let last = pid(&report);
    let other = f.project.with_file_name("other-lookups");
    fs::create_dir(&other).unwrap();
    app.switch_location_home(other.display().to_string())
        .await
        .unwrap();
    assert_eq!(pending.await.unwrap(), Err(McpLookupError::Cancelled));
    gone(last);
    fs::write(gate, "late-release").unwrap();
    assert!(app.mcp_status().await.unwrap().servers.is_empty());
    assert_eq!(
        count(&report, "resources/read"),
        3,
        "no replay after cancel/reload/Location"
    );
    assert_eq!(count(&report, "initialize"), 2);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn mcp11_core_child_foreign_disabled_and_exact_resource_rights_do_not_broaden() {
    let f = Fixture::new();
    let report = f.project.join("rights.events");
    setup(
        &f,
        json!({"peer":entry(&f,"basic",&report,&f.project.join("gate")),
        "disabled":{"type":"local","enabled":false,"command":["{file:missing-program}"]}}),
        json!({"mcp_lookup":{"*":"deny","[\"peer\",\"resources/read\",\"fixture://allowed\"]":"allow"},"read":"allow"}),
        json!({}),
    );
    {
        let db = Db::open(&f.data).unwrap();
        db.create_session("rights-root").unwrap();
        db.create_child_session("rights-root", "rights-child", None, None, None)
            .unwrap();
        db.create_session("rights-foreign").unwrap();
        for id in ["rights-root", "rights-child"] {
            db.set_pref(
                &format!("{}{}", crate::runtime::SESSION_LOCATION_PREFIX, id),
                &f.project.display().to_string(),
            )
            .unwrap();
        }
        db.set_pref(
            &format!("{}rights-foreign", crate::runtime::SESSION_LOCATION_PREFIX),
            "/fixture-other-location",
        )
        .unwrap();
    }
    let (app, guard) = f.spawn().await;
    let (snapshot, _) = wait_status(&app, "peer", McpStatus::Connected).await;
    let allowed = McpLookupOp::ReadResource {
        uri: "fixture://allowed".into(),
    };
    assert_eq!(
        app.mcp_lookup(query(&snapshot, "peer", "rights-child", allowed.clone()))
            .await,
        Err(McpLookupError::ChildSessionUnsupported)
    );
    assert_eq!(
        app.mcp_lookup(query(&snapshot, "peer", "rights-foreign", allowed.clone()))
            .await,
        Err(McpLookupError::SessionMismatch)
    );
    assert_eq!(
        app.mcp_lookup(query(&snapshot, "disabled", "rights-root", allowed.clone()))
            .await,
        Err(McpLookupError::Unavailable)
    );
    assert_eq!(
        app.mcp_lookup(query(
            &snapshot,
            "peer",
            "rights-root",
            McpLookupOp::ReadResource {
                uri: "fixture://denied".into()
            }
        ))
        .await,
        Err(McpLookupError::PermissionDenied)
    );
    assert_eq!(count(&report, "resources/read"), 0);
    app.mcp_lookup(query(&snapshot, "peer", "rights-root", allowed))
        .await
        .unwrap();
    assert_eq!(count(&report, "resources/read"), 1);
    assert_eq!(count(&report, "initialize"), 1);
    let child = pid(&report);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    gone(child);
    assert_eq!(count(&report, "closed"), 1);
}

#[tokio::test]
async fn mcp11_core_combined_generation_metadata_cap_is_fatal_and_all_clients_reaped() {
    let f = Fixture::new();
    let first = f.project.join("first.events");
    let second = f.project.join("second.events");
    setup(
        &f,
        json!({"first":entry(&f,"catalog64",&first,&f.project.join("gate")),
        "second":entry(&f,"catalog64",&second,&f.project.join("gate"))}),
        json!({"mcp_lookup":"allow","read":"allow"}),
        json!({}),
    );
    let (app, guard) = f.spawn().await;
    app.create_session(SessionId("cap-root".into()))
        .await
        .unwrap();
    wait_status(&app, "first", McpStatus::Connected).await;
    let (snapshot, _) = wait_status(&app, "second", McpStatus::Connected).await;
    assert_eq!(
        app.mcp_lookup(query(
            &snapshot,
            "first",
            "cap-root",
            McpLookupOp::ListPrompts
        ))
        .await
        .unwrap()
        .data
        .catalog_entries(),
        64
    );
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(5),
            app.mcp_lookup(query(
                &snapshot,
                "second",
                "cap-root",
                McpLookupOp::ListPrompts
            ))
        )
        .await
        .expect("fatal cap lookup reply"),
        Err(McpLookupError::CatalogLimit)
    );
    let failure = tokio::time::timeout(Duration::from_secs(12), guard.join_diagnostic())
        .await
        .expect("fatal owner cleanup/join")
        .expect_err("generation cap must terminate non-success");
    assert_eq!(
        failure.code,
        oc_core::queries::ServiceCode::CapacityExceeded
    );
    assert_eq!(failure.stage, oc_core::queries::ServiceStage::Admission);
    assert_eq!(
        failure.action,
        oc_core::queries::ServiceAction::ReduceCapacity
    );
    gone(pid(&first));
    gone(pid(&second));
    assert_eq!(count(&first, "closed"), 1);
    assert_eq!(count(&second, "closed"), 1);
    assert_eq!(count(&first, "prompts/list"), 1);
    assert_eq!(count(&second, "prompts/list"), 1);
}
