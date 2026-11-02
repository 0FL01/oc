use super::*;

#[tokio::test]
async fn hot02_retained_failed_unselected_provider_cannot_replace_selected_provider() {
    use oc_core::queries::{ProviderStatus, ServiceKind};
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let path = project.join("opencode.json");
    let mut source = serde_json::json!({
        "model":"fixture/m", "providers":{
            "fixture":{"models":{"m":{}},"settings":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"}},
            "ludka2":{"package":"@ai-sdk/openai","models":{},"settings":{"baseURL":"https://example.invalid/literal","apiKey":"synthetic"}}
        },
        "experimental":{"policies":[{"action":"provider.use","resource":"opencode-go","effect":"deny"},{"action":"provider.use","resource":"openai","effect":"deny"}]}
    });
    std::fs::write(&path, source.to_string()).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    let mut current = composition::load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    current.resolve_credentials(&db).await.unwrap();
    // Apply the normal unselected-view discovery outcome without a network dial.
    assert!(current.accept_view_catalog(
        "ludka2",
        crate::discovery::DiscoveryOutcome {
            models: BTreeMap::new(),
            warnings: Vec::new(),
            attempts: 1,
            replaced: false,
            failure: Some(crate::discovery::DiscoveryFailure::Unauthorized),
        }
    ));
    assert_eq!(
        current
            .readiness_for("ludka2", "exact", None)
            .catalog_status,
        ProviderStatus::Failed
    );
    let runtime = build_runtime(&db, &current).unwrap();
    let initial_generation = runtime.generation_id();
    let mut effective = Effective::from_composition(&current);
    let mut registry = WorkspaceRegistry::bind(
        initial_generation,
        runtime.location(),
        &current.generation,
        workspace_agents(&current),
        skill_metas(&current),
    );
    let mut work = provider_catalog::ProviderWork::idle();
    let (events, _) = broadcast::channel(8);
    source["model"] = "ludka2/exact".into();
    std::fs::write(&path, source.to_string()).unwrap();
    let mut coordinator = Coordinator::default();
    coordinator.ensure(&current).await.unwrap();
    let observation = coordinator.watch.as_ref().unwrap().observation();
    let candidate = tokio::time::timeout(
        Duration::from_secs(3),
        build(&db, BuildInputs::capture(&current), &observation, false),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        candidate.retained_selected,
        "must exercise carried Failed, not rediscovery"
    );
    assert_eq!(
        candidate.next.provider_state.catalog_status,
        ProviderStatus::Failed
    );
    assert_ne!(candidate.next.catalog.provider, current.catalog.provider);
    let (ack, reply) = oneshot::channel();
    let receipt = coordinator
        .finish(
            Ready::Build {
                result: Ok(candidate),
                force: false,
                stamp: runtime.source_stamp(),
                waiters: vec![ack],
            },
            &db,
            &runtime,
            &mut current,
            &mut effective,
            &mut registry,
            &BTreeMap::new(),
            &AtomicU64::new(1),
            &mut work,
            &events,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    receipt.reply();
    assert!(matches!(
        reply.await.unwrap(),
        Err(CoreError::ProviderUnavailable(_))
    ));
    assert_eq!(current.catalog.provider, "fixture");
    assert_eq!(runtime.generation_id(), initial_generation);
    assert_eq!(
        current.reload_issue.as_ref().unwrap().kind,
        ServiceKind::Provider
    );
    coordinator.shutdown().await.unwrap();
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn hot02_observer_failure_restarts_and_failed_restart_releases_force() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let path = project.join("opencode.json");
    let mut config = serde_json::json!({
        "model":"fixture/m", "providers":{"fixture":{"models":{"m":{}},
        "settings":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"}}},
        "experimental":{"policies":[{"action":"provider.use","resource":"opencode-go","effect":"deny"},{"action":"provider.use","resource":"openai","effect":"deny"}]}
    });
    std::fs::write(&path, config.to_string()).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    let mut current = composition::load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    let runtime = build_runtime(&db, &current).unwrap();
    let mut effective = Effective::from_composition(&current);
    let mut registry = WorkspaceRegistry::bind(
        1,
        runtime.location(),
        &current.generation,
        workspace_agents(&current),
        skill_metas(&current),
    );
    let mut work = provider_catalog::ProviderWork::start(&current, &db);
    let (events, _) = broadcast::channel(8);
    let sessions = BTreeMap::new();
    let epoch = AtomicU64::new(1);
    let mut coordinator = Coordinator::default();
    coordinator.ensure(&current).await.unwrap();
    coordinator.watch.as_ref().unwrap().abort();
    let ready = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(ready) = coordinator.wait().await.unwrap() {
                break ready;
            }
        }
    })
    .await
    .unwrap();
    assert!(matches!(ready, Ready::Rewatch));
    coordinator
        .finish(
            ready,
            &db,
            &runtime,
            &mut current,
            &mut effective,
            &mut registry,
            &sessions,
            &epoch,
            &mut work,
            &events,
            None,
        )
        .await
        .unwrap();
    config["agents"] = serde_json::json!({"build":{"system":"AFTER_REWATCH"}});
    std::fs::write(&path, config.to_string()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !current.agents["build"].body.contains("AFTER_REWATCH") {
            coordinator.start(&db, &runtime, &mut current, &events);
            if let Some(ready) = coordinator.wait().await.unwrap()
                && let Some(receipt) = coordinator
                    .finish(
                        ready,
                        &db,
                        &runtime,
                        &mut current,
                        &mut effective,
                        &mut registry,
                        &sessions,
                        &epoch,
                        &mut work,
                        &events,
                        None,
                    )
                    .await
                    .unwrap()
            {
                receipt.reply();
            }
        }
    })
    .await
    .unwrap();

    let (ack, reply) = oneshot::channel();
    coordinator.request(ack);
    coordinator.start(&db, &runtime, &mut current, &events);
    assert!(coordinator.forced());
    let good_inputs = current.watch_inputs.clone();
    let mut bad = current.watch_inputs.targets.iter().next().unwrap().clone();
    bad.path = PathBuf::from("/invalid\0/watch");
    current.watch_inputs.targets.insert(bad);
    coordinator.watch.as_ref().unwrap().abort();
    // Finish the observer transition directly so an already-ready build cannot
    // win this test's select; production wait maps the aborted task to Rewatch.
    coordinator
        .finish(
            Ready::Rewatch,
            &db,
            &runtime,
            &mut current,
            &mut effective,
            &mut registry,
            &sessions,
            &epoch,
            &mut work,
            &events,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(
        reply.await.unwrap(),
        Err(CoreError::Diagnostic(_))
    ));
    assert!(!coordinator.forced());
    assert!(!coordinator.pending && coordinator.watch.is_none());
    assert!(current.reload_issue.is_some());
    // A later explicit request retries observation instead of blocking the
    // submission queue behind a force bit with no live build.
    current.watch_inputs = good_inputs;
    let (ack, mut reply) = oneshot::channel();
    coordinator.request(ack);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            coordinator.start(&db, &runtime, &mut current, &events);
            if let Some(ready) = coordinator.wait().await.unwrap()
                && let Some(receipt) = coordinator
                    .finish(
                        ready,
                        &db,
                        &runtime,
                        &mut current,
                        &mut effective,
                        &mut registry,
                        &sessions,
                        &epoch,
                        &mut work,
                        &events,
                        None,
                    )
                    .await
                    .unwrap()
            {
                receipt.reply();
            }
            if let Ok(result) = reply.try_recv() {
                result.unwrap();
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(current.reload_issue.is_none());
    coordinator.shutdown().await.unwrap();
    work.stop().await.unwrap();
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn hot02_coordinator_keeps_one_build_or_force_and_bounded_waiters() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let path = project.join("opencode.json");
    let config = serde_json::json!({
        "model":"fixture/m", "providers":{"fixture":{"models":{"m":{}},
            "settings":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"}}},
        "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}, {"action": "provider.use", "resource": "openai", "effect": "deny"}]}
    })
    .to_string();
    std::fs::write(&path, &config).unwrap();
    let db = Db::open(&data).unwrap();
    let mut composition = composition::load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    let runtime = build_runtime(&db, &composition).unwrap();
    let (events, _) = broadcast::channel(8);
    let mut coordinator = Coordinator::default();
    coordinator.ensure(&composition).await.unwrap();
    assert!(coordinator.wait().await.unwrap().is_none());

    // Hold the existing build slot, just as the actual held-GET application
    // fixture does. New events/requests cannot spawn a second build or drop force.
    let (release, held) = oneshot::channel::<()>();
    let (active_waiter, active_reply) = oneshot::channel();
    let task = tokio::spawn(async move {
        held.await.unwrap();
        Err(superseded())
    });
    let task_id = task.id();
    coordinator.build = Some(Build {
        task,
        force: true,
        stamp: runtime.source_stamp(),
        waiters: vec![active_waiter],
    });
    let mut replies = vec![active_reply];
    for _ in 1..MAX_QUEUE_ITEMS {
        let (ack, reply) = oneshot::channel();
        coordinator.request(ack);
        replies.push(reply);
    }
    let (overflow, overflow_reply) = oneshot::channel();
    coordinator.request(overflow);
    assert_eq!(overflow_reply.await.unwrap(), Err(CoreError::TurnBusy));
    for n in 0..10 {
        std::fs::write(&path, format!("{config}{}", " ".repeat(n))).unwrap();
        coordinator.start(&db, &runtime, &mut composition, &events);
        assert_eq!(coordinator.build.as_ref().unwrap().task.id(), task_id);
    }
    tokio::time::timeout(Duration::from_secs(2), coordinator.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(coordinator.pending && coordinator.forced());
    assert_eq!(coordinator.waiters.len(), MAX_QUEUE_ITEMS - 1);
    release.send(()).unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(ready) = coordinator.wait().await.unwrap() {
                break ready;
            }
        }
    })
    .await
    .unwrap();
    let mut current = composition;
    let mut effective = Effective::from_composition(&current);
    let mut registry = WorkspaceRegistry::bind(
        1,
        runtime.location(),
        &current.generation,
        workspace_agents(&current),
        skill_metas(&current),
    );
    let mut provider_work = provider_catalog::ProviderWork::start(&current, &db);
    assert!(
        coordinator
            .finish(
                ready,
                &db,
                &runtime,
                &mut current,
                &mut effective,
                &mut registry,
                &BTreeMap::new(),
                &AtomicU64::new(1),
                &mut provider_work,
                &events,
                None
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(coordinator.pending && coordinator.forced());
    assert_eq!(coordinator.waiters.len(), MAX_QUEUE_ITEMS);
    // One latest rebuild owns every accepted waiter; clean shutdown joins it and
    // fails the receipts rather than claiming a source was published.
    coordinator.start(&db, &runtime, &mut current, &events);
    assert!(coordinator.build.as_ref().unwrap().force);
    assert_eq!(
        coordinator.build.as_ref().unwrap().waiters.len(),
        MAX_QUEUE_ITEMS
    );
    coordinator.shutdown().await.unwrap();
    for reply in replies {
        assert_eq!(reply.await.unwrap(), Err(CoreError::TurnBusy));
    }
    provider_work.stop().await.unwrap();
    runtime.shutdown_mcp().await.unwrap();
    assert!(db.list_sessions().unwrap().is_empty());
    assert_eq!(runtime.generation_id(), 1);
}
