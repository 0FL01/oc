use super::*;

struct Held {
    _temp: tempfile::TempDir,
    project: PathBuf,
    db: Db,
    jobs: Arc<Jobs>,
}

impl Held {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let db = Db::open(&temp.path().join("data")).unwrap();
        db.create_bound_session("source", project.to_str().unwrap())
            .unwrap();
        db.create_child_session("source", "child", None, None, None)
            .unwrap();
        db.create_bound_session("foreign", project.to_str().unwrap())
            .unwrap();
        db.begin_turn("turn", "source", "held jobs").unwrap();
        db.begin_turn("child-turn", "child", "held child job")
            .unwrap();
        let jobs = Jobs::new(&db);
        Self {
            _temp: temp,
            project,
            db,
            jobs,
        }
    }

    async fn launch(&self, id: &str, source: &str, foreground: bool, flood: bool) -> i32 {
        let turn = if source == "child" {
            "child-turn"
        } else {
            "turn"
        };
        self.db
            .record_tool_intent(id, source, Some(turn), "shell", "{}")
            .unwrap();
        let command = format!(
            "printf '%s' $$ > {id}.pid; {} printf 'live-{id}'; touch {id}.entered; while [ ! -f {id}.release ]; do sleep .01; done; printf 'final-{id}'; touch {id}.effect",
            if flood {
                "head -c 1100000 /dev/zero | tr '\\000' x; head -c 1100000 /dev/zero | tr '\\000' y >&2; printf live-stderr >&2; "
            } else {
                ""
            }
        );
        let shell = Shell::new(&self.project).unwrap();
        let argv = vec!["/bin/sh".into(), "-c".into(), command.clone()];
        let pinned = shell.pin_cwd(&argv, ".").unwrap();
        self.jobs
            .launch_mode(
                shell,
                BTreeMap::new(),
                argv,
                ".".into(),
                Duration::ZERO,
                pinned,
                Provenance {
                    version: 1,
                    session: source.into(),
                    turn: turn.into(),
                    operation: id.into(),
                    location: self.project.to_string_lossy().into(),
                    generation: 17,
                    output_limits: Default::default(),
                    output_source: "defaults".into(),
                    agent: None,
                    agent_digest: None,
                    model: "gpt-fixture-issuing-request".into(),
                    provider: "fixture".into(),
                    command,
                    cwd: self.project.to_string_lossy().into(),
                    selected_shell: "/bin/sh".into(),
                },
                self.jobs.reserve().unwrap(),
                foreground,
                Vec::new(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !self.project.join(format!("{id}.entered")).exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        std::fs::read_to_string(self.project.join(format!("{id}.pid")))
            .unwrap()
            .parse()
            .unwrap()
    }

    fn release(&self, id: &str) {
        std::fs::write(self.project.join(format!("{id}.release")), b"").unwrap();
    }

    async fn terminal(&self, id: &str, source: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.db.shell_job_phase(source, id).unwrap() != "terminal" {
                self.jobs.changed().await;
            }
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn tool13_conversion_retains_pid_admission_capture_and_final_view_after_removal() {
    let held = Held::new();
    let pid = held.launch("fg", "source", true, true).await;
    let before = held.jobs.running("source").unwrap();
    let live = held.jobs.snapshot("source", "fg").unwrap();
    let parent_cancel = AtomicBool::new(false);
    let mut wait = Box::pin(held.jobs.foreground("source", "fg", &parent_cancel));
    assert!(futures_util::poll!(&mut wait).is_pending());
    assert!(held.jobs.background("source", "fg").unwrap());
    assert!(held.jobs.background("source", "fg").unwrap());
    assert!(wait.await.unwrap().is_none());
    let converted = held.jobs.running("source").unwrap();
    let (events, mut rx) = tokio::sync::broadcast::channel(16);
    held.release("fg");
    held.terminal("fg", "source").await;
    held.jobs.changed().await;
    held.jobs.deliver(&events).unwrap();
    let after = held.jobs.running("source").unwrap();
    let final_view = held.jobs.snapshot("source", "fg").unwrap();
    let late = held.jobs.background("source", "fg").unwrap();
    held.jobs.deliver(&events).unwrap();
    held.jobs.shutdown().await.unwrap();
    assert_eq!(before.len(), 1);
    assert!(!before[0].background);
    assert_eq!(before[0].pid, Some(pid));
    assert_eq!(converted[0].pid, Some(pid));
    assert!(converted[0].background);
    assert_eq!(live.job.generation, 17);
    assert_eq!(live.job.model, "gpt-fixture-issuing-request");
    assert_eq!(live.state, "running");
    assert!(live.stdout_cursor > RETAIN_CAP_BYTES as u64);
    assert!(live.stderr_cursor > RETAIN_CAP_BYTES as u64);
    assert!(live.text.contains("live-stderr"));
    assert!(live.truncated && live.text.contains("live-fg"));
    assert!(live.text.len() <= crate::storage::TOOL_OP_PREVIEW_BYTES);
    assert!(after.is_empty() && !late);
    assert_eq!(final_view.job.pid, Some(pid));
    assert_eq!(final_view.state, "completed");
    assert!(final_view.text.contains("final-fg"));
    assert!(final_view.stdout_cursor > live.stdout_cursor);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(!parent_cancel.load(Ordering::Acquire));
    let notices = std::iter::from_fn(|| rx.try_recv().ok())
        .filter(|event| matches!(event, oc_core::core_app::CoreEvent::ShellNotice(_)))
        .count();
    assert_eq!(notices, 1);
    let conn = rusqlite::Connection::open_with_flags(
        held.db.root().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tool_operations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM events WHERE kind='shell_background'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn tool13_selected_child_kill_does_not_cancel_sibling_parent_or_foreign_source() {
    let held = Held::new();
    let child_pid = held.launch("child-job", "child", false, false).await;
    let sibling_pid = held.launch("sibling", "source", false, false).await;
    let family = held.jobs.running("source").unwrap();
    let foreign = held.jobs.running("foreign").unwrap();
    let wrong_pair = held.jobs.cancel_job("source", "child-job");
    let selected = held.jobs.cancel_job("child", "child-job");
    let repeated = held.jobs.cancel_job("child", "child-job");
    let conversion_after_cancel = held.jobs.background("child", "child-job").unwrap();
    held.terminal("child-job", "child").await;
    let sibling_running = held.jobs.running("source").unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        held.db.root().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let parent_status: String = conn
        .query_row("SELECT status FROM turns WHERE id='turn'", [], |r| r.get(0))
        .unwrap();
    held.release("sibling");
    held.terminal("sibling", "source").await;
    let (events, _) = tokio::sync::broadcast::channel(16);
    held.jobs.deliver(&events).unwrap();
    held.jobs.shutdown().await.unwrap();
    assert_eq!(family.len(), 2);
    assert!(foreign.is_empty());
    assert!(!wrong_pair && selected && repeated && !conversion_after_cancel);
    assert_eq!(sibling_running.len(), 1);
    assert_eq!(sibling_running[0].shell_id, "sibling");
    assert_eq!(parent_status, "started");
    assert_eq!(
        held.jobs.snapshot("child", "child-job").unwrap().state,
        "cancelled"
    );
    assert_eq!(
        held.jobs.snapshot("source", "sibling").unwrap().state,
        "completed"
    );
    assert!(!held.project.join("child-job.effect").exists());
    assert!(held.project.join("sibling.effect").exists());
    for pid in [child_pid, sibling_pid] {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[tokio::test]
async fn tool13_completion_before_conversion_returns_terminal_once_without_notice() {
    let held = Held::new();
    let pid = held.launch("complete", "source", true, false).await;
    held.release("complete");
    held.terminal("complete", "source").await;
    let conversion = held.jobs.background("source", "complete").unwrap();
    let parent_cancel = AtomicBool::new(false);
    let outcome = held
        .jobs
        .foreground("source", "complete", &parent_cancel)
        .await
        .unwrap()
        .unwrap();
    let (events, mut rx) = tokio::sync::broadcast::channel(8);
    held.jobs.deliver(&events).unwrap();
    held.jobs.shutdown().await.unwrap();
    assert!(!conversion);
    assert_eq!(outcome.state, "completed");
    assert!(outcome.stdout.contains("final-complete"));
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, oc_core::core_app::CoreEvent::ShellNotice(_)))
    );
}

#[tokio::test]
async fn tool13_held_owned_shell_output_is_readable_before_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    db.create_bound_session("source", project.to_str().unwrap())
        .unwrap();
    db.begin_turn("turn", "source", "launch").unwrap();
    db.record_tool_intent("operation", "source", Some("turn"), "shell", "{}")
        .unwrap();
    let jobs = Jobs::new(&db);
    let shell = Shell::new(&project).unwrap();
    let argv = vec!["/bin/sh".into(), "-c".into(), "printf live-held-output; touch entered; while [ ! -f release ]; do sleep .01; done; printf final-flush".into()];
    let pinned = shell.pin_cwd(&argv, ".").unwrap();
    jobs.launch(
        shell,
        BTreeMap::new(),
        argv,
        ".".into(),
        Duration::ZERO,
        pinned,
        Provenance {
            version: 1,
            session: "source".into(),
            turn: "turn".into(),
            operation: "operation".into(),
            location: project.to_string_lossy().into(),
            generation: 7,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "fixture".into(),
            provider: "fixture".into(),
            command: "held".into(),
            cwd: project.to_string_lossy().into(),
            selected_shell: "/bin/sh".into(),
        },
        jobs.reserve().unwrap(),
    )
    .await
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !project.join("entered").exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // The process's file barrier does not order the independent drain reader.
    // Observe actual owner output within the SAME original three-second bound.
    let mut live = jobs.output("source", "operation", 0, 1024).unwrap();
    while !live
        .as_ref()
        .is_some_and(|(text, _, _)| text.contains("live-held-output"))
        && Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
        live = jobs.output("source", "operation", 0, 1024).unwrap();
    }
    std::fs::write(project.join("release"), b"").unwrap();
    jobs.changed().await;
    jobs.shutdown().await.unwrap();
    assert!(
        live.is_some_and(|(text, _, _)| text.contains("live-held-output")),
        "actual held supervisor output must be readable before terminal"
    );
}

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
            provenance: None,
            capture: Capture::new(jobs.wake.clone()),
            control: Arc::new(Mutex::new(Control {
                wait_done: true,
                ..Control::default()
            })),
            converted: Arc::default(),
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
        output_limits: Default::default(),
        output_source: "defaults".into(),
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
            provenance: None,
            capture: Capture::new(jobs.wake.clone()),
            control: Arc::new(Mutex::new(Control {
                wait_done: true,
                ..Control::default()
            })),
            converted: Arc::default(),
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
    assert!(
        matches!(receiver.try_recv(), Ok(oc_core::core_app::CoreEvent::ShellChanged { session }) if session.0 == "source")
    );
    jobs.deliver(&events).unwrap();
    assert!(receiver.try_recv().is_err());
    assert!(jobs.shutdown().await.is_err());
}
