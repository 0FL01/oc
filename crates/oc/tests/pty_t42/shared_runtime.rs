use super::*;

#[test]
fn one_tui_new_and_tab_switch_keep_both_root_streams_progressing() {
    let fixture = Fixture::new();
    let mut tui = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "navigation-A"],
        None,
    );
    wait_screen_row(&tui, READY, DEADLINE);
    submit(&mut tui, "srv-progress NAV-A");
    progress_after(&fixture, "NAV-A", 2);
    wait_screen_row(&tui, "NAV-A tick:", DEADLINE);
    let before = fixture.progress.lock().unwrap()["NAV-A"];
    tui.send(b"/new\r");
    wait_saved_home(&tui, &fixture.project_a());
    progress_after(&fixture, "NAV-A", before);
    submit(&mut tui, "srv-progress NAV-B");
    progress_after(&fixture, "NAV-B", 2);
    wait_screen_row(&tui, "NAV-B tick:", DEADLINE);
    for (x, label) in [(10, "NAV-A"), (45, "NAV-B"), (10, "NAV-A")] {
        let before = fixture.progress.lock().unwrap().clone();
        click(&mut tui, x, 1);
        wait_screen_row(&tui, &format!("{label} tick:"), DEADLINE);
        for label in ["NAV-A", "NAV-B"] {
            progress_after(&fixture, label, before[label]);
        }
    }
    // Move the selected, executing A behind B through the production mouse path.
    let before = fixture.progress.lock().unwrap().clone();
    tui.send(b"\x1b[<0;10;1M\x1b[<32;45;1M\x1b[<0;45;1m");
    click(&mut tui, 10, 1);
    wait_screen_row(&tui, "NAV-B tick:", DEADLINE);
    click(&mut tui, 45, 1);
    wait_screen_row(&tui, "NAV-A tick:", DEADLINE);
    for label in ["NAV-A", "NAV-B"] {
        progress_after(&fixture, label, before[label]);
    }
    let requests = fixture.wait_requests(2);
    assert_eq!(requests.len(), 2, "navigation cannot resubmit");
    for prompt in ["srv-progress NAV-A", "srv-progress NAV-B"] {
        assert_eq!(
            requests
                .iter()
                .filter(|body| last_user_text(body).as_deref() == Some(prompt))
                .count(),
            1
        );
    }
    quit(&mut tui);
}
use oc_core::{core_app::CoreEvent, domain::SessionId};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tab_completion_is_authoritative_and_viewed_ack_is_shared_without_changing_selection() {
    let fixture = Fixture::new();
    let mut tui = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "unread-root"],
        None,
    );
    wait_screen_row(&tui, READY, DEADLINE);
    let (source, source_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let (observer, observer_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let session = SessionId("unread-root".into());
    tui.send(b"/new\r");
    wait_screen_row(&tui, "Ask anything", DEADLINE);
    let mut events = source.subscribe();
    let turn = source
        .submit(session.clone(), "completed unread work".into())
        .await
        .unwrap();
    tokio::time::timeout(DEADLINE, async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::Projection { event, .. }
                if matches!(&*event, CoreEvent::TurnFinished { turn: finished, .. } if finished == &turn)) { break; }
        }
    }).await.unwrap();
    let snapshot = observer.workspace(Some(session.clone())).await.unwrap();
    let activity = snapshot
        .activity
        .iter()
        .find(|entry| entry.session == session)
        .unwrap();
    assert_eq!(
        activity.unread(),
        Some(oc_core::queries::SessionIdleOutcome::Activity)
    );
    assert!(!activity.busy);
    wait_screen_row(&tui, "•", DEADLINE);
    assert!(
        render_screen(&tui.snapshot()).rows()[0].contains('•'),
        "background completion paints the real tab indicator"
    );
    let idle_seq = activity.idle_seq.unwrap();
    let deck = observer.tab_deck().await.unwrap();
    assert_eq!(
        observer
            .session_viewed(session.clone(), idle_seq)
            .await
            .unwrap()
            .unread(),
        None
    );
    tokio::time::timeout(DEADLINE, async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::Projection { event, .. }
                if matches!(&*event, CoreEvent::SessionViewed { session: viewed, viewed_seq } if viewed == &session && *viewed_seq == idle_seq)) { break; }
        }
    }).await.unwrap();
    let refreshed = source.workspace(Some(session.clone())).await.unwrap();
    assert_eq!(
        refreshed
            .activity
            .iter()
            .find(|entry| entry.session == session)
            .unwrap()
            .unread(),
        None
    );
    assert_eq!(
        observer.tab_deck().await.unwrap(),
        deck,
        "read acknowledgement is not a deck write or navigation"
    );
    assert_eq!(fixture.wait_requests(1).len(), 1);
    let start = Instant::now();
    while render_screen(&tui.snapshot()).rows()[0].contains('•') {
        assert!(
            start.elapsed() < DEADLINE,
            "viewed broadcast clears the real tab marker after its finite fade"
        );
        std::thread::sleep(POLL);
    }
    quit(&mut tui);
    observer.shutdown().await.unwrap();
    observer_guard.join_diagnostic().await.unwrap();
    source.shutdown().await.unwrap();
    source_guard.join_diagnostic().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn srv04_running_picker_adoption_is_not_a_removed_tab() {
    let fixture = Fixture::new();
    let mut tui = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    wait_screen_row(&tui, "Ask anything", DEADLINE);
    let (source, source_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let (observer, observer_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    // Keep the real owner and two viewers, without an unrelated UI saving the
    // adoption before this deliberately stale (but same-revision) picker.
    quit(&mut tui);
    let original = SessionId("fork-source".into());
    source.create_session(original.clone()).await.unwrap();
    let mut events = source.subscribe();
    let first = source
        .submit(original.clone(), "completed source task".into())
        .await
        .unwrap();
    tokio::time::timeout(DEADLINE, async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::Projection { event, .. }
                if matches!(&*event, CoreEvent::TurnFinished { turn, .. } if turn == &first))
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let page = source
        .history_page(original.clone(), None, None, 24)
        .await
        .unwrap();
    let before = page
        .rows
        .iter()
        .find(|row| row.role == oc_core::session::Role::User)
        .unwrap()
        .id
        .clone();
    observer.session_picker_context(Some(false)).await.unwrap();
    let old = observer
        .save_tab_deck(observer.tab_deck().await.unwrap())
        .await
        .unwrap();
    let target = tokio::time::timeout(DEADLINE, async {
        loop {
            match source.fork_session(original.clone(), before.clone()).await {
                Ok(fork) => break fork.session,
                Err(oc_core::session::CoreError::TurnBusy) => tokio::task::yield_now().await,
                Err(error) => panic!("fork refused: {error}"),
            }
        }
    })
    .await
    .unwrap();
    let turn = source
        .submit(target.clone(), "srv-progress PICKER-ADOPTION".into())
        .await
        .unwrap();
    progress_after(&fixture, "PICKER-ADOPTION", 2);
    let current = observer.tab_deck().await.unwrap();
    assert_eq!(current.revision, old.revision);
    assert!(!old.sessions.contains(&target));
    assert!(current.sessions.contains(&target));
    let opened = observer
        .open_picker_session(target.clone(), String::new(), false, old)
        .await
        .unwrap();
    assert_eq!(opened.deck.active, Some(target.clone()));
    assert!(opened.deck.sessions.contains(&target));
    observer.workspace(Some(target.clone())).await.unwrap();
    observer.cancel_turn(target, turn).await.unwrap();
    observer.shutdown().await.unwrap();
    observer_guard.join_diagnostic().await.unwrap();
    source.shutdown().await.unwrap();
    source_guard.join_diagnostic().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn srv04_stale_turn_and_picker_are_scoped_to_the_issuing_location() {
    let fixture = Fixture::new();
    let mut tui = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    wait_screen_row(&tui, "Ask anything", DEADLINE);
    let (source, source_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let (observer, observer_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let (destination, destination_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_b()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let session = SessionId("controlled-S".into());
    source.create_session(session.clone()).await.unwrap();
    let mut events = source.subscribe();
    let first = source
        .submit(session.clone(), "completed first turn".into())
        .await
        .unwrap();
    tokio::time::timeout(DEADLINE, async {
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if matches!(event, CoreEvent::TurnFinished { turn, .. } if turn == first) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let second = tokio::time::timeout(DEADLINE, async {
        loop {
            match source
                .submit(session.clone(), "srv-progress SCOPED-CONTROL".into())
                .await
            {
                Ok(turn) => break turn,
                // A definite pre-admission refusal is not an ambiguous resend.
                Err(oc_core::session::CoreError::TurnBusy) => tokio::task::yield_now().await,
                Err(error) => panic!("second turn refused: {error}"),
            }
        }
    })
    .await
    .unwrap();
    progress_after(&fixture, "SCOPED-CONTROL", 2);
    observer.workspace(Some(session.clone())).await.unwrap();
    assert_eq!(
        observer.cancel_turn(session.clone(), first).await,
        Err(oc_core::session::CoreError::TurnNotActive),
        "painted T1 must not cancel actual T2"
    );
    let previous = fixture.progress.lock().unwrap()["SCOPED-CONTROL"];
    progress_after(&fixture, "SCOPED-CONTROL", previous);
    let target = SessionId("picker-target".into());
    destination.create_session(target.clone()).await.unwrap();
    let target_turn = destination
        .submit(target.clone(), "srv-progress PICKER-TARGET".into())
        .await
        .unwrap();
    progress_after(&fixture, "PICKER-TARGET", 2);
    let epoch = source.catalog().await.unwrap().chrome.terminal_generation;
    observer.session_picker_context(Some(true)).await.unwrap();
    let old = observer.tab_deck().await.unwrap();
    let opened = observer
        .open_picker_session(target.clone(), String::new(), true, old)
        .await
        .unwrap();
    assert_eq!(opened.session, target);
    assert_eq!(opened.location, fixture.project_b().to_string_lossy());
    assert_eq!(
        observer.catalog().await.unwrap().chrome.location,
        Some(opened.location)
    );
    assert_eq!(
        source.catalog().await.unwrap().chrome.terminal_generation,
        epoch,
        "cross-project picker must not retire the source's running sibling"
    );
    let previous = fixture.progress.lock().unwrap()["SCOPED-CONTROL"];
    progress_after(&fixture, "SCOPED-CONTROL", previous);
    assert!(
        observer
            .cancel_turn(session.clone(), second.clone())
            .await
            .is_err(),
        "a foreign Location cannot acquire S's control"
    );
    observer.workspace(Some(target.clone())).await.unwrap();
    observer.cancel_turn(target, target_turn).await.unwrap();
    let previous = fixture.progress.lock().unwrap()["SCOPED-CONTROL"];
    progress_after(&fixture, "SCOPED-CONTROL", previous);
    source.cancel_turn(session, second).await.unwrap();
    observer.shutdown().await.unwrap();
    observer_guard.join_diagnostic().await.unwrap();
    destination.shutdown().await.unwrap();
    destination_guard.join_diagnostic().await.unwrap();
    source.shutdown().await.unwrap();
    source_guard.join_diagnostic().await.unwrap();
    quit(&mut tui);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn srv04_scoped_native_move_keeps_source_sibling_and_active_destination_running() {
    let fixture = Fixture::new();
    let mut a = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "moving-D"],
        None,
    );
    wait_screen_row(&a, READY, DEADLINE);
    let mut b = PtySession::spawn(
        fixture.clone(),
        &fixture.project_b(),
        &["tui", "--session", "destination-B"],
        None,
    );
    wait_screen_row(&b, READY, DEADLINE);
    b.send(b"srv-progress DESTINATION\r");
    progress_after(&fixture, "DESTINATION", 2);
    let (source, source_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let sibling = SessionId("source-S".into());
    source.create_session(sibling.clone()).await.unwrap();
    source
        .submit(sibling, "srv-progress SOURCE-SIBLING".into())
        .await
        .unwrap();
    progress_after(&fixture, "SOURCE-SIBLING", 2);
    let before = source.catalog().await.unwrap().chrome.terminal_generation;
    let mut events = source.subscribe();
    source.register_approval_consumer(true).await.unwrap();
    let arguments = serde_json::json!({"sessionID":"moving-D", "directory":fixture.project_b()});
    source
        .submit(
            SessionId("moving-D".into()),
            format!("srv-move {arguments}"),
        )
        .await
        .unwrap();
    tokio::time::timeout(DEADLINE, async {
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            match event {
                CoreEvent::SessionMoved {
                    session, directory, ..
                } if session.0 == "moving-D" => {
                    assert_eq!(directory, fixture.project_b().to_string_lossy());
                    break;
                }
                CoreEvent::TurnFailed { session, error, .. } if session.0 == "moving-D" => {
                    panic!("move failed: {error}")
                }
                _ => {}
            }
        }
    })
    .await
    .expect("native move must settle through the actual owner");
    // The tool card already contains B's path before the view adopts B.
    // Wait for the actual Location footer before typing into that view.
    let destination = fixture.project_b();
    let moved = Instant::now();
    loop {
        let rows = render_screen(&a.snapshot()).rows();
        if rows.iter().any(|row| {
            row.trim_start().starts_with(destination.to_str().unwrap())
                && row.contains("tab agents")
        }) {
            break;
        }
        assert!(moved.elapsed() < DEADLINE, "Location not adopted: {rows:?}");
        std::thread::sleep(POLL);
    }
    let progress = fixture.progress.lock().unwrap().clone();
    for label in ["SOURCE-SIBLING", "DESTINATION"] {
        progress_after(&fixture, label, progress[label]);
    }
    assert_eq!(
        source.catalog().await.unwrap().chrome.terminal_generation,
        before,
        "moving D must not replace S's Source"
    );
    let connection = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    let bound: String = connection
        .query_row(
            "SELECT value FROM prefs WHERE key='tui.session_location.moving-D'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bound, fixture.project_b().to_string_lossy());
    source.shutdown().await.unwrap();
    source_guard.join_diagnostic().await.unwrap();
    quit(&mut a);
    quit(&mut b);
    oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
}

pub(super) fn stream(
    socket: &mut TcpStream,
    label: &str,
    stop: &AtomicBool,
    progress: &Mutex<std::collections::BTreeMap<String, usize>>,
) -> std::io::Result<()> {
    stream_paced(socket, label, stop, progress, Duration::from_millis(50), 0)
}

pub(super) fn stream_paced(
    socket: &mut TcpStream,
    label: &str,
    stop: &AtomicBool,
    progress: &Mutex<std::collections::BTreeMap<String, usize>>,
    pace: Duration,
    padding: usize,
) -> std::io::Result<()> {
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
    )?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
    for tick in 1.. {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let delta = serde_json::json!({"type":"response.output_text.delta", "delta":format!("{label} tick:{tick} {}", "x".repeat(padding))});
        write!(socket, "data: {delta}\n\n")?;
        socket.flush()?;
        progress.lock().unwrap().insert(label.into(), tick);
        // Deliberately paced ongoing generation; assertions observe progress.
        std::thread::sleep(pace);
    }
    Ok(())
}

fn progress_after(fixture: &Fixture, label: &str, previous: usize) -> usize {
    let until = Instant::now() + DEADLINE;
    loop {
        let tick = fixture
            .progress
            .lock()
            .unwrap()
            .get(label)
            .copied()
            .unwrap_or(0);
        if tick > previous {
            return tick;
        }
        assert!(
            Instant::now() < until,
            "no new {label} progress after {previous}; turn outcomes: {:?}",
            (|| -> rusqlite::Result<Vec<(String, String, Option<String>)>> {
                let db = rusqlite::Connection::open_with_flags(
                    fixture.data_dir().join("oc.sqlite"),
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                db.prepare("SELECT session_id,status,substr(result,1,512) FROM turns ORDER BY rowid LIMIT 16")?
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .collect()
            })()
        );
        std::thread::sleep(POLL);
    }
}

struct OwnedProcess(Child);
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn srv03_headless_run_survives_last_tui_then_sigint_joins_owner() {
    let fixture = Fixture::new();
    let mut tui = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "interactive"],
        None,
    );
    wait_screen_row(&tui, READY, DEADLINE);
    tui.send(b"srv-progress INTERACTIVE\r");
    progress_after(&fixture, "INTERACTIVE", 2);
    let run = fixture
        .command_in(&fixture.project_a())
        .args(["run", "--session", "finite-run", "srv-progress RUN"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut run = OwnedProcess(run);
    let until = Instant::now() + DEADLINE;
    while fixture
        .progress
        .lock()
        .unwrap()
        .get("RUN")
        .copied()
        .unwrap_or_default()
        <= 2
    {
        assert!(
            run.0.try_wait().unwrap().is_none(),
            "headless ELF exited before entering the held provider response"
        );
        assert!(
            Instant::now() < until,
            "headless request did not enter provider"
        );
        std::thread::sleep(POLL);
    }
    quit(&mut tui);
    let previous = fixture.progress.lock().unwrap()["RUN"];
    progress_after(&fixture, "RUN", previous);
    assert!(
        run.0.try_wait().unwrap().is_none(),
        "last TUI must not cancel the independent run lease"
    );
    // SAFETY: signal only this fixture's still-owned child, never an external PID.
    assert_eq!(unsafe { libc::kill(run.0.id() as i32, libc::SIGINT) }, 0);
    let until = Instant::now() + DEADLINE;
    let status = loop {
        if let Some(status) = run.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < until,
            "finite run/owner did not join after SIGINT"
        );
        std::thread::sleep(POLL);
    };
    assert_eq!(status.code(), Some(130));
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    drop(db);
    let connection = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    for (session, expected) in [("interactive", "interrupted"), ("finite-run", "cancelled")] {
        let state: String = connection
            .query_row(
                "SELECT status FROM turns WHERE session_id=?1 ORDER BY rowid DESC LIMIT 1",
                [session],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, expected);
    }
}

#[test]
fn srv02_two_pty_live_handoff_and_global_abc_last_close() {
    let fixture = Fixture::new();
    std::fs::remove_file(fixture.project_b().join("opencode.json")).unwrap();
    let project_c = fixture.root.path().join("c");
    std::fs::create_dir(&project_c).unwrap();
    let mut a = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "shared-A"],
        None,
    );
    wait_screen_row(&a, READY, DEADLINE);
    a.send(b"srv-progress A\r");
    progress_after(&fixture, "A", 2);
    let mut mirror = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    wait_screen_row(&mirror, "A tick:", DEADLINE);
    mirror.send(b"LOCAL-UNSENT-DRAFT");
    quit(&mut a);
    let before = *fixture.progress.lock().unwrap().get("A").unwrap();
    progress_after(&fixture, "A", before);
    wait_screen_row(&mirror, "LOCAL-UNSENT-DRAFT", DEADLINE);
    let mut b = PtySession::spawn(
        fixture.clone(),
        &fixture.project_b(),
        &["tui", "--session", "shared-B"],
        None,
    );
    wait_screen_row(&b, READY, DEADLINE);
    b.send(b"srv-progress B\r");
    progress_after(&fixture, "B", 2);
    let mut c = PtySession::spawn(
        fixture.clone(),
        &project_c,
        &["tui", "--session", "shared-C"],
        None,
    );
    wait_screen_row(&c, READY, DEADLINE);
    c.send(b"srv-progress C\r");
    progress_after(&fixture, "C", 2);
    // Normal quit with a local unsent draft uses the actual keybinding, not kill.
    // Clear exactly this deliberately typed local draft. Ctrl+U is not the
    // pinned composer's delete-line binding and must not be assumed by fixtures.
    mirror.send(&vec![0x7f; "LOCAL-UNSENT-DRAFT".len()]);
    quit(&mut mirror);
    quit(&mut b);
    let observed = fixture.progress.lock().unwrap().clone();
    for label in ["A", "B", "C"] {
        progress_after(&fixture, label, observed[label]);
    }
    let mut reattached = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    wait_screen_row(&reattached, "A tick:", DEADLINE);
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|body| !title::is_title(body)
                && last_user_text(body).as_deref() == Some("srv-progress A"))
            .count(),
        1,
        "reattach must not restart accepted execution"
    );
    quit(&mut reattached);
    quit(&mut c);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    drop(db);
    let connection = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    let interrupted: i64 = connection
        .query_row(
            "SELECT count(*) FROM turns WHERE status='interrupted'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        interrupted, 3,
        "last TUI settles every Location as Interrupted"
    );
    let requests = fixture.requests.lock().unwrap().len();
    let mut reopened = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    wait_screen_row(&reopened, "interrupted", DEADLINE);
    quit(&mut reopened);
    assert_eq!(
        fixture.requests.lock().unwrap().len(),
        requests,
        "reopen is history only, never auto-resume"
    );
}

#[test]
fn sessions_list_stdout_ids_only() {
    let fixture = Fixture::new();
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    db.create_session("s-a").unwrap();
    db.create_session("s-b").unwrap();
    drop(db);
    let output = fixture
        .command_in(&fixture.project_a())
        .args(["sessions", "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("s-a") && text.contains("s-b"));
    assert!(output.stderr.is_empty());
}

fn projectless_owner(fixture: &Fixture) -> (OwnedProcess, std::os::unix::net::UnixStream) {
    use std::os::{
        fd::AsRawFd,
        unix::{net::UnixStream, process::CommandExt as _},
    };
    let (mut startup, inherited) = UnixStream::pair().unwrap();
    startup.set_read_timeout(Some(DEADLINE)).unwrap();
    let fd = inherited.as_raw_fd();
    let mut command = fixture.command_in(&fixture.project_a());
    command
        .arg("__native-owner")
        .arg("--bootstrap-fd")
        .arg(fd.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let inherit = move || {
        // SAFETY: this descriptor is the fixture-created startup socket.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, 0) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    };
    // SAFETY: the closure performs only the async-signal-safe descriptor change.
    unsafe {
        command.pre_exec(inherit);
    }
    let owner = OwnedProcess(command.spawn().unwrap());
    drop(inherited);
    // This fixture exercises the same projectless cold-owner admission as auth.
    let intent =
        serde_json::to_vec(&serde_json::json!({"version":8,"lease":"Auth","project":null}))
            .unwrap();
    startup
        .write_all(&(intent.len() as u32).to_be_bytes())
        .unwrap();
    startup
        .write_all(&(intent.len() as u32).to_be_bytes())
        .unwrap();
    startup.write_all(&intent).unwrap();
    let mut header = [0; 4];
    startup.read_exact(&mut header).unwrap();
    let length = u32::from_be_bytes(header) as usize;
    assert!(length < 8192);
    let mut bytes = vec![0; length];
    let mut offset = 0;
    while offset < length {
        startup.read_exact(&mut header).unwrap();
        let frame = u32::from_be_bytes(header) as usize;
        assert!(frame > 0 && frame <= length - offset);
        startup
            .read_exact(&mut bytes[offset..offset + frame])
            .unwrap();
        offset += frame;
    }
    let ready: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        ready["Ready"]["pid"],
        owner.0.id(),
        "actual ELF storage owner"
    );
    (owner, startup)
}

#[test]
fn srv01_lost_startup_intent_joins_owner_without_any_ready_client() {
    let fixture = Fixture::new();
    let (mut owner, startup) = projectless_owner(&fixture);
    assert!(matches!(
        oc_adapters::storage::Db::open(&fixture.data_dir()),
        Err(oc_adapters::storage::StorageError::DataRootBusy)
    ));
    drop(startup);
    let until = Instant::now() + DEADLINE;
    loop {
        if let Some(status) = owner.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "startup intent EOF must join clean zero-client shutdown"
            );
            break;
        }
        assert!(
            Instant::now() < until,
            "lost initiator left an initial-zero owner alive"
        );
        std::thread::sleep(POLL);
    }
    assert!(!fixture.data_dir().join("native-owner.json").exists());
    assert!(
        oc_adapters::storage::Db::open(&fixture.data_dir())
            .unwrap()
            .list_sessions()
            .unwrap()
            .is_empty()
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
}

/// A gap repair is captured against the view's projection floor. Deltas that
/// arrive while it is read wait for it; applying them first made every repair
/// stale and requested another workspace read until the stream ended.
#[test]
fn reattach_during_fast_stream_settles_workspace_repairs() {
    let fixture = Fixture::new();
    let mut first = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "burst-repair"],
        None,
    );
    wait_screen_row(&first, READY, DEADLINE);
    first.send(b"srv-burst BURST\r");
    // A large live turn keeps each repair open while many deltas land.
    progress_after(&fixture, "BURST", 2_000);
    first.child.kill().unwrap();
    first.child.wait().unwrap();
    let path = fixture.root.path().join("burst-repair-metrics.json");
    let mut second = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], Some(&path));
    wait_screen_row(&second, "BURST tick:", DEADLINE);
    // About half a second of 1 ms deltas while the reattached view repairs.
    let attached = progress_after(&fixture, "BURST", 0);
    progress_after(&fixture, "BURST", attached + 500);
    wait_screen_row(&second, "BURST tick:", DEADLINE);
    quit(&mut second);
    let reads = metrics(&path)["workspace_reads"].as_u64().unwrap();
    assert!(
        reads <= 8,
        "workspace repairs did not settle: {reads} reads"
    );
}

#[test]
fn srv03_client_crash_handoff_last_crash_grace_then_quit_settles_interrupted() {
    let fixture = Fixture::new();
    let mut first = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "crash-handoff"],
        None,
    );
    wait_screen_row(&first, READY, DEADLINE);
    first.send(b"srv-progress CLIENT-EOF\r");
    progress_after(&fixture, "CLIENT-EOF", 2);
    let mut second = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    wait_screen_row(&second, "CLIENT-EOF tick:", DEADLINE);
    let registration: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture.data_dir().join("native-owner.json")).unwrap(),
    )
    .unwrap();
    let pid = registration["pid"].as_u64().unwrap() as u32;
    assert_eq!(
        std::fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
        Path::new(BIN)
    );
    // SAFETY: open a process-exit descriptor for the verified fixture ELF only.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    assert!(fd >= 0);
    // SAFETY: the successful syscall returned this newly owned descriptor.
    let owner = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    first.child.kill().unwrap();
    first.child.wait().unwrap();
    let previous = fixture.progress.lock().unwrap()["CLIENT-EOF"];
    progress_after(&fixture, "CLIENT-EOF", previous);
    wait_screen_row(&second, "CLIENT-EOF tick:", DEADLINE);
    assert!(matches!(
        oc_adapters::storage::Db::open(&fixture.data_dir()),
        Err(oc_adapters::storage::StorageError::DataRootBusy)
    ));
    second.child.kill().unwrap();
    second.child.wait().unwrap();
    // The last viewer vanished without Release: the owner keeps the interactive
    // turn running for the reattachment grace instead of settling it at once.
    let previous = fixture.progress.lock().unwrap()["CLIENT-EOF"];
    progress_after(&fixture, "CLIENT-EOF", previous);
    let mut poll = libc::pollfd {
        fd: owner.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: poll borrows this live local descriptor and the initialized pollfd.
    let alive = unsafe { libc::poll(&mut poll, 1, 0) };
    assert_eq!(alive, 0, "a crashed last viewer may still reattach");
    let mut third = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    wait_screen_row(&third, "CLIENT-EOF tick:", DEADLINE);
    // An explicit quit keeps the immediate last-disconnect contract.
    quit(&mut third);
    // SAFETY: poll borrows this live local descriptor and the initialized pollfd.
    let exited = unsafe { libc::poll(&mut poll, 1, DEADLINE.as_millis() as i32) };
    assert_eq!(
        exited, 1,
        "the last released viewer must not leave an unowned daemon"
    );
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let connection = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    let state: String = connection
        .query_row(
            "SELECT status FROM turns WHERE session_id='crash-handoff' ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "interrupted");
    assert_eq!(db.list_sessions().unwrap(), ["crash-handoff"]);
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|body| !title::is_title(body)
                && last_user_text(body).as_deref() == Some("srv-progress CLIENT-EOF"))
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn srv01_owner_elf_two_bridges_and_auth_lease() {
    use oc_core::queries::{TerminalAction, TerminalReceipt, TerminalSize};
    let fixture = Fixture::new();
    let (mut owner, startup) = projectless_owner(&fixture);
    let (first, first_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    let (second, second_guard, _) = oc_adapters::native::attach(
        &fixture.data_dir(),
        Some(&fixture.project_a()),
        oc_adapters::native::Lease::Tui,
    )
    .await
    .unwrap();
    drop(startup);
    let session = SessionId("shared-native-rpc".into());
    first.create_session(session.clone()).await.unwrap();
    assert_eq!(
        second.probe_session(session.clone()).await.unwrap(),
        oc_core::queries::SessionProbe::Root
    );
    assert!(matches!(
        oc_adapters::storage::Db::open(&fixture.data_dir()),
        Err(oc_adapters::storage::StorageError::DataRootBusy)
    ));
    let catalog = first.catalog().await.unwrap();
    let original_size = TerminalSize { rows: 24, cols: 80 };
    let TerminalReceipt::Created(terminal) = first
        .terminal(
            session.clone(),
            TerminalAction::Create {
                location: catalog.chrome.location.clone().unwrap(),
                generation: catalog.chrome.terminal_generation,
                size: original_size,
            },
        )
        .await
        .unwrap()
    else {
        panic!("terminal creation receipt");
    };
    second
        .terminal(
            session.clone(),
            TerminalAction::Select(Some(terminal.target.clone())),
        )
        .await
        .unwrap();
    let replacement_size = TerminalSize {
        rows: 30,
        cols: 100,
    };
    second
        .terminal(
            session.clone(),
            TerminalAction::Resize {
                target: terminal.target.clone(),
                size: replacement_size,
            },
        )
        .await
        .unwrap();
    let TerminalReceipt::Snapshot(passive) = second
        .terminal(
            session.clone(),
            TerminalAction::Snapshot(terminal.target.clone()),
        )
        .await
        .unwrap()
    else {
        panic!("passive snapshot");
    };
    assert_eq!(
        passive.size, original_size,
        "a passive mirror must not resize the real PTY"
    );
    assert!(
        second
            .terminal(
                session.clone(),
                TerminalAction::Input {
                    target: terminal.target.clone(),
                    bytes: b"not-controlled\r".to_vec()
                }
            )
            .await
            .is_err()
    );
    let mut stale = terminal.target.clone();
    stale.generation += 1;
    assert!(
        second
            .terminal(
                session.clone(),
                TerminalAction::Resize {
                    target: stale,
                    size: original_size
                }
            )
            .await
            .is_err()
    );

    let (auth, auth_guard, _) =
        oc_adapters::native::attach(&fixture.data_dir(), None, oc_adapters::native::Lease::Auth)
            .await
            .unwrap();
    let added = auth
        .provider_accounts(
            "openai".into(),
            Some(oc_core::queries::AccountAction::AddKey {
                label: "RPC fixture".into(),
                key: oc_core::queries::KeyInput::new("not-a-real-key".into()),
            }),
        )
        .await
        .unwrap();
    let created = added.created.unwrap();
    assert!(added.accounts.iter().any(|account| account.id == created));
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "auth attachment needs no generation"
    );
    auth.shutdown().await.unwrap();
    auth_guard.join_diagnostic().await.unwrap();
    first.shutdown().await.unwrap();
    first_guard.join_diagnostic().await.unwrap();
    second
        .terminal(
            session.clone(),
            TerminalAction::Select(Some(terminal.target.clone())),
        )
        .await
        .unwrap();
    second
        .terminal(
            session.clone(),
            TerminalAction::Resize {
                target: terminal.target.clone(),
                size: replacement_size,
            },
        )
        .await
        .unwrap();
    // Applied acknowledges the bounded terminal control queue, not a completed
    // ioctl. Observe the actual drain's snapshot before asserting physical size.
    tokio::time::timeout(DEADLINE, async {
        loop {
            let TerminalReceipt::Snapshot(controlled) = second
                .terminal(
                    session.clone(),
                    TerminalAction::Snapshot(terminal.target.clone()),
                )
                .await
                .unwrap()
            else {
                panic!("controller snapshot");
            };
            if controlled.size == replacement_size {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("controller's resize must reach the real PTY");
    second
        .terminal(session.clone(), TerminalAction::Remove(terminal.target))
        .await
        .unwrap();
    let mut events = second.subscribe();
    let turn = second
        .submit(session, "owner survives first disconnect".into())
        .await
        .unwrap();
    tokio::time::timeout(DEADLINE, async {
        loop { if matches!(match events.recv().await.unwrap() { CoreEvent::Projection { event, .. } => *event, event => event }, CoreEvent::TurnFinished { turn: done, .. } if done == turn) { break; } }
    }).await.unwrap();
    second.shutdown().await.unwrap();
    second_guard.join_diagnostic().await.unwrap();
    assert!(
        owner.0.wait().unwrap().success(),
        "last lease joins owner exit"
    );
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let scope =
        oc_adapters::auth::AuthScope::admit("openai", oc_adapters::auth::OPENAI_BASE_URL).unwrap();
    assert!(
        db.credential_accounts(scope.namespace())
            .unwrap()
            .iter()
            .any(|account| account.id == created)
    );
}

#[test]
fn srv01_two_actual_tuis_attach_to_one_storage_owner() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let mut first = PtySession::spawn(fixture.clone(), &project, &[], None);
    let mut second = PtySession::spawn(fixture.clone(), &project, &[], None);
    // Both actual ELFs start before either is ready: the Db election winner
    // retains its lineage while the loser waits for the private endpoint.
    wait_screen_row(&first, "Ask anything", DEADLINE);
    let until = Instant::now() + DEADLINE;
    loop {
        let rows = render_screen(&second.snapshot()).rows().join("\n");
        assert!(
            !rows.contains("Data root busy"),
            "second ordinary TUI must attach: {rows}"
        );
        if rows.contains("Ask anything") {
            break;
        }
        assert!(
            Instant::now() < until,
            "second TUI did not become ready: {rows}"
        );
        std::thread::sleep(POLL);
    }
    quit(&mut first);
    second.send(b"shared owner survives first quit\r");
    wait_screen_row(&second, "echo: shared owner survives first quit", DEADLINE);
    wait_idle(&second);
    quit(&mut second);
    assert_eq!(journal_counts(&fixture).0, 1);
}
