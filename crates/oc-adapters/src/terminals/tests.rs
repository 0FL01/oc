use super::*;
use std::time::Instant;

fn fixture() -> (tempfile::TempDir, Db, BTreeMap<String, String>, TerminalRef) {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let db = Db::open(&dir.path().join("data")).unwrap();
    db.create_session("session").unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), project.to_string_lossy().into()),
        ("SHELL".into(), "/bin/bash".into()),
        ("OC_API_KEY".into(), "synthetic-secret".into()),
        ("UNLISTED".into(), "synthetic-secret".into()),
    ]);
    let source = TerminalRef {
        id: String::new(),
        session: SessionId("session".into()),
        location: project.to_string_lossy().into(),
        generation: 7,
    };
    (dir, db, env, source)
}
fn text(snapshot: &TerminalSnapshot) -> String {
    snapshot
        .cells
        .chunks(snapshot.size.cols as usize)
        .map(|row| {
            row.iter()
                .map(|c| {
                    if c.text.is_empty() {
                        " "
                    } else {
                        c.text.as_str()
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn wait(owner: &Terminals, target: &TerminalRef, expected: &str) -> TerminalSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let s = owner.snapshot(&target.session, target).unwrap();
        if text(&s).contains(expected) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected:?}, actual {:?}",
            text(&s)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn term01_real_pty_identity_environment_raw_control_resize_and_replay() {
    let (_dir, db, env, source) = fixture();
    let mut owner = Terminals::new(&db).unwrap();
    assert!(owner.inventory(&source.session).unwrap().entries.is_empty());
    let entry = owner
        .create(source.clone(), &env, TerminalSize::default())
        .unwrap();
    let target = &entry.target;
    assert_eq!(entry.cwd, source.location);
    assert_eq!(entry.shell, "/bin/bash");
    assert!(
        ProcessIdentity::read(entry.pid, db.root())
            .unwrap()
            .matches(db.root())
    );
    let child_env = std::fs::read(format!("/proc/{}/environ", entry.pid)).unwrap();
    assert!(
        !child_env
            .windows(b"synthetic-secret".len())
            .any(|w| w == b"synthetic-secret")
    );
    owner.select(&target.session, Some(target)).unwrap();
    let checkpoint = owner.snapshot(&target.session, target).unwrap();
    assert!(checkpoint.ready);
    owner
        .input(
            &target.session,
            target,
            b"printf 'one-%s\\n' OK; pwd\r".to_vec(),
        )
        .unwrap();
    let frame = wait(&owner, target, "one-OK");
    assert!(text(&frame).contains(&entry.cwd));
    let replay = owner
        .replay(&target.session, target, checkpoint.output_cursor)
        .unwrap();
    assert!(replay.reset.is_none());
    assert_eq!(replay.bytes.len() as u64, replay.next - replay.from);
    assert!(replay.next > checkpoint.output_cursor);
    owner
        .resize(&target.session, target, TerminalSize { rows: 30, cols: 91 })
        .unwrap();
    owner
        .input(&target.session, target, b"stty size\r".to_vec())
        .unwrap();
    let s = wait(&owner, target, "30 91");
    assert_eq!(s.size, TerminalSize { rows: 30, cols: 91 });
    owner
        .input(&target.session, target, b"sleep 30\r".to_vec())
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    owner.input(&target.session, target, vec![3]).unwrap();
    owner
        .input(
            &target.session,
            target,
            b"printf 'control-%s\\n' OK\r".to_vec(),
        )
        .unwrap();
    wait(&owner, target, "control-OK");
    let mut stale = target.clone();
    stale.generation += 1;
    assert!(owner.input(&target.session, &stale, b"x".to_vec()).is_err());
    assert!(
        owner
            .input(&SessionId("alien".into()), target, b"x".to_vec())
            .is_err()
    );
    owner.select(&target.session, None).unwrap();
    assert!(owner.inventory(&target.session).unwrap().selected.is_none());
    assert!(Path::new(&format!("/proc/{}", entry.pid)).exists());
    owner.remove(&target.session, target).unwrap();
    assert!(!Path::new(&format!("/proc/{}", entry.pid)).exists());
    assert!(owner.inventory(&target.session).unwrap().entries.is_empty());
}

#[test]
fn term01_vt_unicode_styles_escape_flood_and_gap_are_bounded() {
    let (_dir, db, env, source) = fixture();
    let mut owner = Terminals::new(&db).unwrap();
    let e = owner.create(source, &env, TerminalSize::default()).unwrap();
    let t = &e.target;
    owner
        .input(
            &t.session,
            t,
            b"printf '\\033[2J\\033[H\\033[31mRED\\033[0m \\347\\225\\214\\033[2;1Hd\\157ne'\r"
                .to_vec(),
        )
        .unwrap();
    let s = wait(&owner, t, "done");
    assert_eq!(s.cells[0].text, "R");
    assert_eq!(s.cells[0].fg, TerminalColor::Indexed(1));
    assert_eq!(s.cells[4].text, "界");
    assert!(s.cells[5].wide_continuation);
    owner.input(&t.session,t,b"python3 -c \"import sys;sys.stdout.write('\\x1b]52;c;'+('x'*1000000)+'\\x07'+('a'*100000)+'EN'+'D')\"\r".to_vec()).unwrap();
    let s = wait(&owner, t, "END");
    assert!(s.control_strings_dropped > 0);
    let replay = owner.replay(&t.session, t, 0).unwrap();
    assert!(replay.reset.is_some());
    assert!(replay.bytes.is_empty());
    let state = owner.live[&t.id].state.lock().unwrap();
    assert!(state.ring.len() <= RING_BYTES);
    assert!(state.gate.bytes.len() <= 1024);
    drop(state);
    owner.shutdown().unwrap();
    assert!(!Path::new(&format!("/proc/{}", e.pid)).exists());
}

#[test]
fn term01_two_hidden_pty_shutdown_and_stale_recovery_never_replay() {
    let (_dir, db, env, source) = fixture();
    let mut owner = Terminals::new(&db).unwrap();
    let a = owner
        .create(source.clone(), &env, TerminalSize::default())
        .unwrap();
    let b = owner
        .create(source.clone(), &env, TerminalSize::default())
        .unwrap();
    owner.select(&source.session, Some(&a.target)).unwrap();
    owner.select(&source.session, None).unwrap();
    owner
        .input(
            &b.target.session,
            &b.target,
            b"printf 'hidden-%s\\n' OK\r".to_vec(),
        )
        .unwrap();
    wait(&owner, &b.target, "hidden-OK");
    let rows = owner.inventory(&source.session).unwrap();
    assert_eq!(rows.entries[0].target, a.target);
    assert_eq!(rows.entries[1].target, b.target);
    owner.shutdown().unwrap();
    for e in [&a, &b] {
        assert!(!Path::new(&format!("/proc/{}", e.pid)).exists());
    }
    // A stale serialized identity cannot authorize killing even a same-UID PID.
    let mut saved = serde_json::to_value(
        ProcessIdentity::read(std::process::id() as i32, db.root()).unwrap_or_else(|| {
            // Test runner is not necessarily a group leader; create a fresh owned PTY.
            let mut p = Terminals::new(&db).unwrap();
            let e = p
                .create(source.clone(), &env, TerminalSize::default())
                .unwrap();
            let identity = ProcessIdentity::read(e.pid, db.root()).unwrap();
            p.shutdown().unwrap();
            identity
        }),
    )
    .unwrap();
    saved["start_ticks"] = serde_json::json!(0);
    let identity: ProcessIdentity = serde_json::from_value(saved).unwrap();
    assert!(!identity.matches(db.root()));
    let recovered = Terminals::new(&db).unwrap();
    assert!(recovered.live.is_empty());
}

#[test]
fn term01_escape_gate_restarts_aborted_csi_before_osc() {
    let (_dir, _db, _env, source) = fixture();
    let entry = TerminalEntry {
        target: source,
        shell: "sh".into(),
        cwd: "/fixture".into(),
        pid: 42,
        title: "sh".into(),
        foreground: None,
        state: TerminalState::Running,
        exit: None,
    };
    let mut s = Screen::new(entry, TerminalSize::default());
    s.output(b"\x1b[2;\x1b]52;c;");
    s.output(&vec![b'x'; 200_000]);
    s.output(b"\x07OK");
    assert!(s.gate.bytes.is_empty());
    assert_eq!(s.gate.dropped, 1);
    assert!(text(&s.snapshot()).contains("OK"));
}

#[test]
fn term01_cleanup_publication_failure_is_sticky_and_retains_recovery() {
    let (_dir, db, env, source) = fixture();
    let mut owner = Terminals::new(&db).unwrap();
    let entry = owner.create(source, &env, TerminalSize::default()).unwrap();
    owner
        .select(&entry.target.session, Some(&entry.target))
        .unwrap();
    // A real publication error after the actual process was joined/reaped must
    // never turn into success on a second shutdown or discard durable identity.
    let fault = rusqlite::Connection::open(db.root().join("oc.sqlite")).unwrap();
    fault.execute_batch("CREATE TRIGGER fail_terminal BEFORE UPDATE ON terminals BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(owner.remove(&entry.target.session, &entry.target).is_err());
    assert!(!Path::new(&format!("/proc/{}", entry.pid)).exists());
    assert!(owner.shutdown().is_err());
    assert!(owner.shutdown().is_err());
    assert_eq!(
        fault
            .query_row("SELECT live FROM terminals", [], |r| r.get::<_, i32>(0))
            .unwrap(),
        1
    );
    fault.execute_batch("DROP TRIGGER fail_terminal;").unwrap();
    drop(owner);
    let mut recovered = Terminals::new(&db).unwrap();
    assert!(
        recovered
            .inventory(&entry.target.session)
            .unwrap()
            .entries
            .is_empty()
    );
    let (live, raw) = fault
        .query_row("SELECT live,entry FROM terminals", [], |r| {
            Ok((r.get::<_, i32>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap();
    assert_eq!(live, 0);
    assert_eq!(
        serde_json::from_str::<TerminalEntry>(&raw).unwrap().state,
        TerminalState::Interrupted
    );
    assert!(
        db.selected_terminal(&entry.target.session.0)
            .unwrap()
            .is_none()
    );
    assert!(signal(0, libc::SIGTERM).is_err());
    // Invalid signal on this exact fixture-owned runner group has no effect;
    // exercise a real non-ESRCH syscall refusal rather than ignoring errno.
    // SAFETY: getpgrp reads the fixture runner's group and sends no signal itself.
    assert!(signal(unsafe { libc::getpgrp() }, -999).is_err());
}
