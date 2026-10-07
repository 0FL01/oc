use super::*;
use std::fs;

#[tokio::test]
async fn tool21_review_short_interrupted_capture_is_explicit_without_falsifying_exit() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    db.create_bound_session("s", project.to_str().unwrap())
        .unwrap();
    db.begin_turn("t", "s", "capture").unwrap();
    db.record_tool_intent("op", "s", Some("t"), "shell", "{}")
        .unwrap();
    let jobs = Jobs::new(&db);
    let shell = Shell::new(&project).unwrap();
    // The leader exits0; a descendant retains both pipes past the existing
    // drain window. Group disposal loses its future suffix, not leader effects.
    let argv = vec![
        "/bin/sh".into(),
        "-c".into(),
        "printf 'short prefix\\n'; printf effect >> effect; (sleep 30; printf lost_suffix) &"
            .into(),
    ];
    let pinned = shell.pin_cwd(&argv, ".").unwrap();
    jobs.launch_mode(
        shell,
        BTreeMap::new(),
        argv,
        ".".into(),
        Duration::from_secs(40),
        pinned,
        provenance(&project, "op"),
        jobs.reserve().unwrap(),
        true,
        Vec::new(),
    )
    .await
    .unwrap();
    let outcome = jobs
        .foreground("s", "op", &AtomicBool::new(false))
        .await
        .unwrap()
        .unwrap();
    jobs.shutdown().await.unwrap();
    let resource = db.output_for_operation("op").unwrap().unwrap();
    assert_eq!(
        resource.state,
        crate::storage::tool_output::CaptureState::Interrupted
    );
    assert!(resource.admitted_bytes < 4096 && resource.admitted_lines < 20);
    assert_eq!(outcome.exit, Some(0));
    assert_eq!(outcome.state, "completed");
    assert!(
        outcome.capture_failure.is_none(),
        "producer interruption is not a logging fault"
    );
    assert!(
        outcome.output_prepared,
        "short incomplete capture must still be prepared"
    );
    let presentation = outcome.output_presentation.as_deref().unwrap();
    assert_eq!(presentation.body, "short prefix\n");
    assert!(presentation.generated_guidance && !presentation.body_limited);
    assert_eq!(
        presentation.capture.as_ref().unwrap().state,
        crate::storage::tool_output::CaptureState::Interrupted
    );
    let streams = presentation.shell.as_ref().unwrap();
    assert_eq!(streams.stdout, "short prefix\n");
    assert_eq!(streams.exit, Some(0));
    assert!(!streams.stdout_limited && !streams.stderr_limited);
    assert!(
        !presentation.body.contains("capture Interrupted")
            && !presentation.body.contains("lost_suffix")
    );
    let (state, text) = outcome.tool_result();
    assert_eq!(state, "completed");
    assert!(text.contains("capture Interrupted") && text.contains(&resource.path));
    assert!(!text.contains("lost_suffix"));
    db.record_tool_outcome("op", state, Some(&text)).unwrap();
    assert!(
        db.list_tool_ops("s").unwrap()[0]
            .output
            .as_ref()
            .unwrap()
            .contains("capture Interrupted")
    );
    assert_eq!(
        fs::read_to_string(project.join("effect")).unwrap(),
        "effect"
    );
    drop(jobs);
    drop(db);
    let db = Db::open(&temp.path().join("data")).unwrap();
    assert_eq!(db.shell_job_outcome("s", "op").unwrap().exit, Some(0));
    assert_eq!(
        db.shell_job_outcome("s", "op")
            .unwrap()
            .output_presentation
            .as_deref(),
        Some(presentation)
    );
    assert!(
        db.list_tool_ops("s").unwrap()[0]
            .output
            .as_ref()
            .unwrap()
            .contains("capture Interrupted")
    );
    assert_eq!(
        fs::read_to_string(project.join("effect")).unwrap(),
        "effect"
    );
}

#[test]
fn tool21_review_safe_publication_order_has_original_raw_read_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    db.create_bound_session("s", "/project").unwrap();
    db.begin_turn("t", "s", "capture").unwrap();
    db.record_tool_intent("op", "s", Some("t"), "shell", "{}")
        .unwrap();
    let p = provenance(Path::new("/project"), "op");
    let capture = Capture::new(Arc::default());
    capture
        .stream
        .lock()
        .unwrap()
        .begin(&db, &p, vec!["split-secret-value".into()]);
    capture.ingest(Stream::Stdout, b"split-secret-", false, true); // raw1, unpublished
    capture.ingest(Stream::Stderr, b"stderr observed second\n", false, true); // raw2, immediate
    let active = db.output_for_operation("op").unwrap().unwrap();
    let mut reader = db.open_tool_output("s", &active.path).unwrap();
    assert_eq!(
        reader.byte_page(0, 4096).unwrap().0,
        "[stderr]\nstderr observed second\n"
    );
    capture.ingest(Stream::Stdout, b"value\n", false, true); // raw3, releases raw1 carry
    capture.ingest(Stream::Stdout, &[], true, true);
    capture.ingest(Stream::Stderr, &[], true, true);
    let (_, resource, failed) = capture.stream.lock().unwrap().finish(&db, &p, false);
    assert!(!failed);
    let resource = resource.unwrap();
    let mut reader = db.open_tool_output("s", &resource.path).unwrap();
    assert_eq!(
        reader.byte_page(0, 4096).unwrap().0,
        "[stderr]\nstderr observed second\n[stdout]\n[redacted]\n"
    );
    let facts = serde_json::to_value(resource.shell.unwrap()).unwrap();
    assert!(
        facts["format"]
            .as_str()
            .unwrap()
            .contains("normalized/redacted publication")
    );
    assert_eq!(facts["stdout_first_read"], 1);
    assert_eq!(facts["stdout_last_read"], 3);
    assert_eq!(facts["stderr_first_read"], 2);
    assert_eq!(facts["stderr_last_read"], 2);
    assert_eq!(facts["stdout_carry_releases"], 1);
    assert_eq!(facts["stderr_carry_releases"], 0);
    // UTF-8 alone has the same ordering domain: raw stdout comes first,
    // safe complete stderr publishes first, then the complete stdout character.
    db.record_tool_intent("utf8", "s", Some("t"), "shell", "{}")
        .unwrap();
    let p = provenance(Path::new("/project"), "utf8");
    let capture = Capture::new(Arc::default());
    capture.stream.lock().unwrap().begin(&db, &p, Vec::new());
    capture.ingest(Stream::Stdout, b"\xe4\xba", false, true);
    capture.ingest(Stream::Stderr, b"stderr before safe UTF-8\n", false, true);
    capture.ingest(Stream::Stdout, b"\x8c\n", false, true);
    capture.ingest(Stream::Stdout, &[], true, true);
    capture.ingest(Stream::Stderr, &[], true, true);
    let (_, resource, failed) = capture.stream.lock().unwrap().finish(&db, &p, false);
    assert!(!failed);
    let resource = resource.unwrap();
    let mut reader = db.open_tool_output("s", &resource.path).unwrap();
    assert_eq!(
        reader.byte_page(0, 4096).unwrap().0,
        "[stderr]\nstderr before safe UTF-8\n[stdout]\n二\n"
    );
    let facts = resource.shell.unwrap();
    assert_eq!(
        (facts.stdout_first_read, facts.stdout_last_read),
        (Some(1), Some(3))
    );
    assert_eq!(
        (facts.stderr_first_read, facts.stderr_last_read),
        (Some(2), Some(2))
    );
    assert_eq!(facts.stdout_carry_releases, 1);
}

fn provenance(project: &Path, operation: &str) -> Provenance {
    Provenance {
        version: 1,
        session: "s".into(),
        turn: "t".into(),
        operation: operation.into(),
        location: project.to_string_lossy().into(),
        generation: 17,
        output_limits: crate::tools::output::Limits {
            max_lines: 20,
            max_bytes: 4096,
        },
        output_source: "captured fixture".into(),
        agent: None,
        agent_digest: None,
        model: "fixture".into(),
        provider: "fixture".into(),
        command: "owned fixture".into(),
        cwd: project.to_string_lossy().into(),
        selected_shell: "/bin/sh".into(),
    }
}

#[test]
fn tool21_shell_split_secret_utf8_observed_streams_live_lease_and_io_loss() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    db.create_bound_session("s", "/project").unwrap();
    db.begin_turn("t", "s", "capture").unwrap();
    db.record_tool_intent("op", "s", Some("t"), "shell", "{}")
        .unwrap();
    let p = provenance(Path::new("/project"), "op");
    let capture = Capture::new(Arc::default());
    capture
        .stream
        .lock()
        .unwrap()
        .begin(&db, &p, vec!["split-secret-value".into()]);
    capture.ingest(Stream::Stdout, b"prefix split-secret-", false, true);
    capture.ingest(Stream::Stderr, b"independent stderr\n", false, true);
    capture.ingest(Stream::Stdout, b"value \xe4\xba", false, true);
    capture.ingest(Stream::Stdout, b"\x8c\nnext row\0\n", false, true);
    capture.ingest(Stream::Stdout, &[], true, true);
    capture.ingest(Stream::Stderr, &[], true, true);
    let active = db.output_for_operation("op").unwrap().unwrap();
    let mut lease = db.open_tool_output("s", &active.path).unwrap();
    let live = lease.byte_page(0, 4096).unwrap().0;
    assert!(live.contains("[stdout]") && live.contains("[stderr]"));
    assert!(live.contains("[redacted] 二\nnext row\n"));
    assert!(!live.contains("split-secret") && !capture.text().contains("split-secret"));
    assert_eq!(
        db.expire_tool_outputs(i64::MAX / 2).unwrap(),
        0,
        "active protected"
    );
    let (_, resource, failed) = capture.stream.lock().unwrap().finish(&db, &p, false);
    assert!(!failed);
    let resource = resource.unwrap();
    assert_eq!(resource.id, active.id);
    assert_eq!(
        resource.state,
        crate::storage::tool_output::CaptureState::Complete
    );
    let facts = resource.shell.unwrap();
    assert_eq!(facts.observed_chunks, 4);
    assert_eq!(
        facts.stdout_bytes,
        "prefix [redacted] 二\nnext row\n".len() as u64
    );
    assert_eq!(facts.stderr_bytes, "independent stderr\n".len() as u64);
    assert_eq!(
        db.expire_tool_outputs(i64::MAX / 2).unwrap(),
        0,
        "reader protected across rename"
    );
    drop(lease);
    assert_eq!(db.expire_tool_outputs(i64::MAX / 2).unwrap(), 1);

    db.record_tool_intent("io", "s", Some("t"), "shell", "{}")
        .unwrap();
    let p = provenance(Path::new("/project"), "io");
    let capture = Capture::new(Arc::default());
    capture.stream.lock().unwrap().begin(&db, &p, Vec::new());
    capture.ingest(Stream::Stdout, b"durable before IO\n", false, true);
    fs::rename(db.root().join("tool-output"), db.root().join("held-output")).unwrap();
    fs::create_dir(db.root().join("tool-output")).unwrap();
    for _ in 0..300 {
        capture.ingest(Stream::Stdout, &[b'x'; 8192], false, true);
    }
    capture.ingest(Stream::Stdout, &[], true, true);
    capture.ingest(Stream::Stderr, &[], true, true);
    let (tail, resource, failed) = capture.stream.lock().unwrap().finish(&db, &p, false);
    assert!(failed && resource.is_none());
    assert!(tail.len() <= output::RECENT_CAP);
    assert!(capture.stdout.lock().unwrap().bytes.len() <= output::RECENT_CAP);
    assert_eq!(capture.stdout.lock().unwrap().total, 18 + 300 * 8192);
    fs::remove_dir(db.root().join("tool-output")).unwrap();
    fs::rename(db.root().join("held-output"), db.root().join("tool-output")).unwrap();
    let failed = db.output_for_operation("io").unwrap().unwrap();
    assert_eq!(failed.state, crate::storage::tool_output::CaptureState::Io);
    assert!(failed.admitted_bytes > 2 * 1024 * 1024 && failed.bytes < 1024);
    db.record_tool_intent("secret-budget", "s", Some("t"), "shell", "{}")
        .unwrap();
    let p = provenance(Path::new("/project"), "secret-budget");
    let capture = Capture::new(Arc::default());
    capture
        .stream
        .lock()
        .unwrap()
        .begin(&db, &p, vec!["Z".repeat(65537)]);
    capture.ingest(Stream::Stdout, b"unsafe raw producer text", false, true);
    capture.ingest(Stream::Stdout, &[], true, true);
    capture.ingest(Stream::Stderr, &[], true, true);
    let (tail, resource, failed) = capture.stream.lock().unwrap().finish(&db, &p, false);
    assert!(failed && resource.is_none() && tail.is_empty());
    assert!(!capture.text().contains("unsafe raw producer text"));
    assert_eq!(capture.stdout.lock().unwrap().total, 24);
}

#[tokio::test]
async fn tool21_shell_cap_and_shared_quota_keep_draining_effects_and_bounded_hot_state() {
    for (quota, want) in [
        (
            2 * 1024 * 1024 * 1024,
            crate::storage::tool_output::CaptureState::ArtifactCap,
        ),
        (32768, crate::storage::tool_output::CaptureState::Quota),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let db = Db::open_with_quota(&temp.path().join("data"), quota).unwrap();
        db.create_bound_session("s", project.to_str().unwrap())
            .unwrap();
        db.begin_turn("t", "s", "capture").unwrap();
        db.record_tool_intent("op", "s", Some("t"), "shell", "{}")
            .unwrap();
        let jobs = Jobs::new(&db);
        let command = "head -c 20971520 /dev/zero | tr '\\000' x; printf 'effect' >> effect; printf 'final flush\\n'";
        let shell = Shell::new(&project).unwrap();
        let argv = vec!["/bin/sh".into(), "-c".into(), command.into()];
        let pinned = shell.pin_cwd(&argv, ".").unwrap();
        jobs.launch_mode(
            shell,
            BTreeMap::new(),
            argv,
            ".".into(),
            Duration::from_secs(120),
            pinned,
            provenance(&project, "op"),
            jobs.reserve().unwrap(),
            true,
            Vec::new(),
        )
        .await
        .unwrap();
        let outcome = jobs
            .foreground("s", "op", &AtomicBool::new(false))
            .await
            .unwrap()
            .unwrap();
        let memory = jobs
            .work
            .lock()
            .unwrap()
            .get("op")
            .unwrap()
            .capture
            .stdout
            .lock()
            .unwrap()
            .bytes
            .len();
        jobs.shutdown().await.unwrap();
        let resource = db.output_for_operation("op").unwrap().unwrap();
        assert_eq!(resource.state, want);
        assert!(resource.bytes <= crate::storage::tool_output::CAP && resource.bytes <= quota);
        assert!(
            resource.admitted_bytes > 20 * 1024 * 1024 && resource.admitted_bytes > resource.bytes
        );
        assert_eq!(outcome.exit, Some(0));
        assert_eq!(
            outcome.state, "failed",
            "logging failure is separate from exit0"
        );
        assert!(outcome.stdout.contains("final flush"));
        assert!(outcome.tool_result().1.len() <= crate::tools::output::SERVED_CAP);
        assert!(memory <= output::RECENT_CAP);
        assert_eq!(
            fs::read_to_string(project.join("effect")).unwrap(),
            "effect"
        );
    }
}

#[tokio::test]
async fn tool21_shell_stalled_publication_has_no_queue_and_selected_cancel_joins() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    db.create_bound_session("s", project.to_str().unwrap())
        .unwrap();
    db.begin_turn("t", "s", "capture").unwrap();
    db.record_tool_intent("op", "s", Some("t"), "shell", "{}")
        .unwrap();
    let jobs = Jobs::new(&db);
    let shell = Shell::new(&project).unwrap();
    let argv=vec!["/bin/sh".into(),"-c".into(),"touch entered; while [ ! -f start ]; do sleep .01; done; while :; do printf 'flood line 012345678901234567890123456789\\n'; done".into()];
    let pinned = shell.pin_cwd(&argv, ".").unwrap();
    jobs.launch_mode(
        shell,
        BTreeMap::new(),
        argv,
        ".".into(),
        Duration::ZERO,
        pinned,
        provenance(&project, "op"),
        jobs.reserve().unwrap(),
        true,
        Vec::new(),
    )
    .await
    .unwrap();
    let entered = tokio::time::timeout(Duration::from_secs(3), async {
        while !project.join("entered").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    if entered.is_err() {
        jobs.shutdown().await.unwrap();
        panic!("producer did not enter");
    }
    // Directed offline fault connection, not another production Db owner/pool.
    // Hold publication's SQLite write barrier while both real drains backpressure.
    let blocker = rusqlite::Connection::open(db.root().join("oc.sqlite")).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    fs::write(project.join("start"), b"").unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let capture = jobs.work.lock().unwrap().get("op").unwrap().capture.clone();
    let stdout_len = capture.stdout.lock().unwrap().bytes.len();
    let stderr_len = capture.stderr.lock().unwrap().bytes.len();
    let started = Instant::now();
    let cancelled = jobs.cancel_job("s", "op");
    let cancel_elapsed = started.elapsed();
    blocker.execute_batch("ROLLBACK").unwrap();
    drop(blocker);
    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        jobs.foreground("s", "op", &AtomicBool::new(false)),
    )
    .await;
    jobs.shutdown().await.unwrap();
    let outcome = outcome.unwrap().unwrap().unwrap();
    assert!(
        cancelled && cancel_elapsed < Duration::from_millis(100),
        "selected cancel cannot wait on the writer"
    );
    assert!(stdout_len <= output::RECENT_CAP && stderr_len <= output::RECENT_CAP);
    assert_eq!(outcome.state, "cancelled");
    assert_eq!(
        outcome.capture.unwrap().state,
        crate::storage::tool_output::CaptureState::Interrupted
    );
    assert!(capture.stdout.lock().unwrap().done && capture.stderr.lock().unwrap().done);
}

#[tokio::test]
async fn tool21_shell_logging_event_failure_keeps_durable_exit_and_effect_without_replay() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    db.create_bound_session("s", project.to_str().unwrap())
        .unwrap();
    db.begin_turn("t", "s", "capture").unwrap();
    db.record_tool_intent("op", "s", Some("t"), "shell", "{}")
        .unwrap();
    let fault = rusqlite::Connection::open(db.root().join("oc.sqlite")).unwrap();
    fault.execute_batch("CREATE TRIGGER publication_fault BEFORE UPDATE OF name ON tool_output_resources BEGIN SELECT RAISE(ABORT,'fixture publication fault'); END;
        CREATE TRIGGER logging_fault BEFORE INSERT ON events WHEN NEW.kind='tool_output_execution' BEGIN SELECT RAISE(ABORT,'fixture logging fault'); END;").unwrap();
    drop(fault);
    let jobs = Jobs::new(&db);
    let shell = Shell::new(&project).unwrap();
    let argv = vec![
        "/bin/sh".into(),
        "-c".into(),
        "printf effect >> effect; head -c 70000 /dev/zero | tr '\\000' x; exit 17".into(),
    ];
    let pinned = shell.pin_cwd(&argv, ".").unwrap();
    jobs.launch_mode(
        shell,
        BTreeMap::new(),
        argv,
        ".".into(),
        Duration::from_secs(30),
        pinned,
        provenance(&project, "op"),
        jobs.reserve().unwrap(),
        true,
        Vec::new(),
    )
    .await
    .unwrap();
    let result = jobs.foreground("s", "op", &AtomicBool::new(false)).await;
    let shutdown = jobs.shutdown().await;
    let outcome = db.shell_job_outcome("s", "op").unwrap();
    assert!(result.is_err() && shutdown.is_err());
    assert_eq!(outcome.exit, Some(17));
    assert_eq!(outcome.state, "failed");
    assert_eq!(
        outcome.capture_failure,
        Some(crate::storage::tool_output::CaptureState::RegisterFailure)
    );
    assert!(outcome.capture.is_none() && outcome.stdout.contains("no usable path"));
    assert_eq!(
        fs::read_to_string(project.join("effect")).unwrap(),
        "effect"
    );
}

#[tokio::test]
async fn tool21_shell_captures_distant_stdout_before_recent_discard() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    db.create_bound_session("s", project.to_str().unwrap())
        .unwrap();
    db.begin_turn("t", "s", "capture").unwrap();
    db.record_tool_intent("op", "s", Some("t"), "shell", "{}")
        .unwrap();
    let jobs = Jobs::new(&db);
    let command = "printf effect >> effect; python3 -c \"import sys;sys.stdout.write('ordinary padded row 0123456789012345678901234567890123456789\\n'*22000+'DISTANT_STDOUT 二\\n'+'ordinary padded row 0123456789012345678901234567890123456789\\n'*22000);sys.stderr.write('stderr fact\\n')\"";
    let shell = Shell::new(&project).unwrap();
    let argv = vec!["/bin/sh".into(), "-c".into(), command.into()];
    let pinned = shell.pin_cwd(&argv, ".").unwrap();
    jobs.launch_mode(
        shell,
        BTreeMap::new(),
        argv,
        ".".into(),
        Duration::from_secs(30),
        pinned,
        Provenance {
            version: 1,
            session: "s".into(),
            turn: "t".into(),
            operation: "op".into(),
            location: project.to_string_lossy().into(),
            generation: 17,
            output_limits: crate::tools::output::Limits {
                max_lines: 20,
                max_bytes: 4096,
            },
            output_source: "captured fixture".into(),
            agent: None,
            agent_digest: None,
            model: "fixture".into(),
            provider: "fixture".into(),
            command: command.into(),
            cwd: project.to_string_lossy().into(),
            selected_shell: "/bin/sh".into(),
        },
        jobs.reserve().unwrap(),
        true,
        Vec::new(),
    )
    .await
    .unwrap();
    let outcome = jobs
        .foreground("s", "op", &AtomicBool::new(false))
        .await
        .unwrap()
        .unwrap();
    jobs.shutdown().await.unwrap();
    let resource = db.output_for_operation("op").unwrap().unwrap();
    assert_eq!(
        resource.state,
        crate::storage::tool_output::CaptureState::Complete,
        "recent-ring truncation must not mean producer loss"
    );
    let (_, text) = outcome.tool_result();
    assert!(text.len() <= crate::tools::output::SERVED_CAP);
    assert!(!text.contains("DISTANT_STDOUT"));
    assert!(text.contains(&resource.path));
    let mut reader = db.open_tool_output("s", &resource.path).unwrap();
    assert!(reader.resource.bytes > 2 * 1024 * 1024);
    let mut buffer = [0; 8192];
    let mut found = false;
    let mut carry = String::new();
    loop {
        let n = reader.file.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        carry.push_str(&String::from_utf8_lossy(&buffer[..n]));
        found |= carry.contains("DISTANT_STDOUT 二");
        carry = carry[carry.ceil_char_boundary(carry.len().saturating_sub(64))..].into();
    }
    assert!(
        found,
        "stdout fact beyond old1MiB must be captured before discard"
    );
    assert_eq!(
        fs::read_to_string(project.join("effect")).unwrap(),
        "effect"
    );
}
