use super::*;

#[tokio::test]
async fn tool13_early_wake_cannot_strand_join_or_capacity_when_owner_returns_idle() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(temp.path()).unwrap();
    let jobs = Jobs::new(&db);
    let slots = (0..ACTIVE_JOB_CAP)
        .map(|_| jobs.reserve().unwrap())
        .collect::<Vec<_>>();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let wake = jobs.wake.clone();
    let task = tokio::task::spawn_blocking(move || -> Result<(), StorageError> {
        wake.notify_one(); // Deliberately before the task can finish.
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        Ok(())
    });
    let mut slots = slots.into_iter();
    jobs.work.lock().unwrap().insert(
        "barrier".into(),
        Work {
            _slot: slots.next().unwrap(),
            session: "source".into(),
            cancel: Arc::new(AtomicBool::new(false)),
            completion: async move { matches!(task.await, Ok(Ok(()))) }
                .boxed()
                .shared(),
        },
    );
    let remaining = slots.collect::<Vec<_>>();
    entered_rx.await.unwrap();
    jobs.changed().await; // Consume the premature notification.
    let (events, _) = tokio::sync::broadcast::channel(8);
    jobs.deliver(&events).unwrap();
    assert_eq!(jobs.work.lock().unwrap().len(), 1);
    assert!(jobs.reserve().is_none());
    // Re-enter the idle owner's actual wait, then permit the worker to return.
    // No second notify, model turn, timer polling or scheduler-yield workaround.
    let mut changed = Box::pin(jobs.changed());
    assert!(futures_util::poll!(&mut changed).is_pending());
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), changed)
        .await
        .unwrap();
    jobs.deliver(&events).unwrap();
    assert!(jobs.work.lock().unwrap().is_empty());
    drop(remaining);
    let reusable = (0..ACTIVE_JOB_CAP)
        .map(|_| jobs.reserve().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(reusable.len(), ACTIVE_JOB_CAP);
    jobs.shutdown().await.unwrap();
}

#[tokio::test]
async fn tool13_cancelled_owner_wait_keeps_worker_then_failed_join_is_durable_and_sticky() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(temp.path()).unwrap();
    db.create_bound_session("source", "/project").unwrap();
    db.begin_turn("turn", "source", "launch").unwrap();
    db.record_tool_intent("operation", "source", Some("turn"), "shell", "{}")
        .unwrap();
    let jobs = Jobs::new(&db);
    db.admit_shell_job(&Provenance {
        version: 1,
        session: "source".into(),
        turn: "turn".into(),
        operation: "operation".into(),
        location: "/project".into(),
        generation: 1,
        agent: None,
        agent_digest: None,
        model: "synthetic".into(),
        provider: "fixture".into(),
        command: "never executed".into(),
        cwd: "/project".into(),
        selected_shell: "/bin/sh".into(),
    })
    .unwrap();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let task = tokio::task::spawn_blocking(move || -> Result<(), StorageError> {
        release_rx.recv().unwrap();
        panic!("directed supervisor join failure");
    });
    let cancel = Arc::new(AtomicBool::new(false));
    jobs.work.lock().unwrap().insert(
        "operation".into(),
        Work {
            _slot: jobs.reserve().unwrap(),
            session: "source".into(),
            cancel: cancel.clone(),
            completion: async move { matches!(task.await, Ok(Ok(()))) }
                .boxed()
                .shared(),
        },
    );
    // Abort the actual teardown wait after it arms cancellation. Work and its
    // shared raw join remain in the sole owner until a subsequent wait joins it.
    let mut shutdown = Box::pin(jobs.shutdown());
    assert!(futures_util::poll!(&mut shutdown).is_pending());
    assert!(cancel.load(Ordering::Acquire));
    drop(shutdown);
    assert_eq!(jobs.work.lock().unwrap().len(), 1);
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), jobs.changed())
        .await
        .unwrap();
    let (events, mut receiver) = tokio::sync::broadcast::channel(8);
    jobs.deliver(&events).unwrap();
    let oc_core::core_app::CoreEvent::ShellNotice(notice) = receiver.recv().await.unwrap() else {
        panic!("failed join must produce its durable unknown notice");
    };
    assert_eq!(notice.state, "unknown");
    assert_eq!(notice.session.0, "source");
    assert!(jobs.work.lock().unwrap().is_empty());
    jobs.deliver(&events).unwrap();
    assert!(receiver.try_recv().is_err());
    assert!(jobs.shutdown().await.is_err());
}
