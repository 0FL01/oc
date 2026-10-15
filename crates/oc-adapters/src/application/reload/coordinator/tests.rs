use super::*;

#[tokio::test]
async fn hot02_coordinator_keeps_one_build_or_force_and_bounded_waiters() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let path = project.join("opencode.json");
    let config = serde_json::json!({
        "model":"fixture/m", "disabled_providers":["opencode-go","openai"],
        "provider":{"fixture":{"models":{"m":{}},
            "options":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"}}}
    })
    .to_string();
    std::fs::write(&path, &config).unwrap();
    let db = Db::open(&data).unwrap();
    let composition = composition::load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    let runtime = build_runtime(&db, &composition).unwrap();
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
        coordinator.start(&db, &runtime, &composition);
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
    let (events, _) = broadcast::channel(8);
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
    coordinator.start(&db, &runtime, &current);
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
