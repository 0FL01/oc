use super::*;

#[test]
fn exited_group_keeps_pid_reserved_without_waiting_out_a_zombie() {
    let process = Command::new("/bin/true").process_group(0).spawn().unwrap();
    let mut child = OwnedChild {
        process,
        grace: Duration::from_secs(2),
        reaped: false,
    };
    let pid = child.process.id() as i32;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !exited_without_reap(pid).unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(
        group_alive(pid),
        "the unreaped leader must still reserve its PGID"
    );
    let started = Instant::now();
    kill_group(pid, child.grace);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "a zombie cannot use a graceful shutdown window"
    );
    assert!(
        exited_without_reap(pid).unwrap(),
        "no early reap or PID reuse window"
    );
    assert!(child.process.wait().unwrap().success());
    child.reaped = true;
}

#[test]
fn running_group_retains_the_full_term_grace_and_is_killed_before_reap() {
    let dir = tempfile::tempdir().unwrap();
    let ready = dir.path().join("ready");
    let process = Command::new("/bin/sh")
        .arg("-c")
        .arg("trap '' TERM; printf ready > \"$1\"; exec sleep 30")
        .arg("fixture")
        .arg(&ready)
        .process_group(0)
        .spawn()
        .unwrap();
    let mut child = OwnedChild {
        process,
        grace: Duration::from_millis(200),
        reaped: false,
    };
    let pid = child.process.id() as i32;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let started = Instant::now();
    kill_group(pid, child.grace);
    assert!(
        started.elapsed() >= child.grace,
        "a live TERM-ignoring process retains its grace"
    );
    assert_eq!(child.process.wait().unwrap().signal(), Some(libc::SIGKILL));
    child.reaped = true;
}
