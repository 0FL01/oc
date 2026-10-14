use super::*;

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
                catalogs: [None, None, None],
                catalog_pending: [false; 3],
            },
        )]),
        generation: McpGeneration::empty(2, owner.shared.wake.clone()),
        work: JoinSet::new(),
        retiring: Vec::new(),
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
        owner.completion_warnings(&binding, &initial),
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
            owner.completion_warnings(&mismatch, &initial),
            [original.to_string()]
        );
    }
    let replacement = Arc::new(McpOwner::new("old-location", 7));
    let mut foreign = McpGeneration::empty(7, replacement.shared.wake.clone());
    foreign.degraded = vec![changed];
    replacement.shared.publication.write().unwrap().request = Arc::new(foreign);
    assert_eq!(
        replacement.completion_warnings(&binding, &initial),
        [original.to_string()],
        "same Location/generation but different owner instance mixed warnings"
    );
    // Diagnostic observation cannot clear or demote an existing fatal result.
    owner.shared.publication.write().unwrap().fatal = Some(RuntimeError::McpShutdown);
    let _ = owner.completion_warnings(&binding, &initial);
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
