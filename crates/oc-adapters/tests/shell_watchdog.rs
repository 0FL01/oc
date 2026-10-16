//! AUD27: pathological children must not hang the shell supervisor.
//!
//! The driver re-executes this test binary as a *separate subprocess* and
//! watches it with a hard wall-clock bound, so a supervisor regression that
//! deadlocks cannot hang the test runner: it is killed and reported. Each
//! case runs the production `Shell::execute` against a real child:
//!
//! - `stdin-unread`: child never reads 64 KiB of stdin;
//! - `flood`: child writes several MiB to stdout and stderr;
//! - `leader-exit-descendant-pipe`: leader exits while a descendant keeps
//!   the pipes open;
//! - `term-ignoring`: child ignores TERM so only the group KILL ends it.

use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use oc_adapters::shell::{Shell, ShellLimits};

const CHILD_TEST: &str = "aud27_watchdog_child";
const WATCHDOG: Duration = Duration::from_secs(30);

fn limits(timeout_ms: u64) -> ShellLimits {
    ShellLimits {
        timeout: Duration::from_millis(timeout_ms),
        kill_grace: Duration::from_millis(300),
        retain_cap: 1024 * 1024,
    }
}

#[test]
fn aud27_pathological_children_are_bounded_under_watchdog() {
    for case in [
        "stdin-unread",
        "flood",
        "leader-exit-descendant-pipe",
        "term-ignoring",
    ] {
        let output = run_case(case);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "case {case} failed: {stdout}{stderr}"
        );
        assert!(
            stdout.contains(&format!("WATCHDOG_OK {case}")),
            "case {case} produced no marker: {stdout}{stderr}"
        );
    }
}

fn run_case(case: &str) -> std::process::Output {
    let exe = std::env::current_exe().expect("current test binary");
    let mut child = Command::new(exe)
        .arg(CHILD_TEST)
        .arg("--exact")
        .arg("--nocapture")
        .env("OC_SHELL_WATCHDOG_CASE", case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn watchdog child");
    let deadline = Instant::now() + WATCHDOG;
    loop {
        if child.try_wait().expect("poll watchdog child").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("watchdog: case {case} exceeded {WATCHDOG:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().expect("watchdog child output")
}

/// Runs inside the watchdog subprocess; a no-op under an ordinary test run.
#[test]
fn aud27_watchdog_child() {
    let Ok(case) = std::env::var("OC_SHELL_WATCHDOG_CASE") else {
        return;
    };
    let tmp = tempfile::tempdir().expect("temp");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    let shell = Shell::new(&project).expect("shell");
    let parent: BTreeMap<String, String> = std::env::vars().collect();
    let no_cancel = AtomicBool::new(false);
    let started = Instant::now();
    match case.as_str() {
        "stdin-unread" => {
            // The child sleeps without ever reading its stdin; the writer
            // must not stall the supervisor and the deadline must win.
            let payload = vec![b'x'; 64 * 1024];
            let out = shell
                .execute(
                    &parent,
                    &["sleep".to_string(), "30".to_string()],
                    ".",
                    Some(&payload),
                    limits(700),
                    &no_cancel,
                )
                .expect("stdin case");
            assert!(out.timed_out && !out.cancelled, "{out:?}");
        }
        "flood" => {
            // ~4 MiB per stream against a 1 MiB retain cap: bounded, flagged,
            // and no deadlock between the two pipes.
            let out = shell
                .run_sh(
                    &parent,
                    "i=0\nwhile [ $i -lt 200 ]\ndo\nhead -c 20000 /dev/zero\necho\nhead -c 20000 /dev/zero >&2\ni=$((i+1))\ndone",
                    ".",
                    None,
                    limits(30_000),
                    &no_cancel,
                )
                .expect("flood case");
            assert_eq!(out.code, Some(0));
            assert!(out.stdout_truncated, "stdout cap not reached");
            assert!(out.stderr_truncated, "stderr cap not reached");
        }
        "leader-exit-descendant-pipe" => {
            // The leader exits immediately; the backgrounded descendant keeps
            // both pipes open, so the drains cannot finish on their own. The
            // descendant would also create the marker after a second: the
            // bounded teardown must kill the owned group before that.
            let marker = tmp.path().join("descendant-marker");
            let script = format!(
                "(sleep 1; touch '{}') & exit 0",
                marker.to_string_lossy().replace('\'', "'\\''")
            );
            let out = shell
                .execute(
                    &parent,
                    &["sh".to_string(), "-c".to_string(), script],
                    ".",
                    None,
                    limits(500),
                    &no_cancel,
                )
                .expect("leader-exit case");
            assert_eq!(out.code, Some(0), "{out:?}");
            assert!(!out.timed_out && !out.cancelled, "{out:?}");
            std::thread::sleep(Duration::from_millis(1500));
            assert!(
                !marker.exists(),
                "owned descendant survived the group teardown"
            );
        }
        "term-ignoring" => {
            // SIG_IGN on TERM is inherited by the sleeping child, so the
            // group only dies at the KILL stage of the teardown ladder.
            let out = shell
                .execute(
                    &parent,
                    &[
                        "sh".to_string(),
                        "-c".to_string(),
                        "trap '' TERM\nsleep 30".to_string(),
                    ],
                    ".",
                    None,
                    limits(500),
                    &no_cancel,
                )
                .expect("term case");
            assert!(out.timed_out && !out.cancelled, "{out:?}");
        }
        other => panic!("unknown watchdog case {other}"),
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(15),
        "case {case} was not bounded: {elapsed:?}"
    );
    println!("WATCHDOG_OK {case} elapsed_ms={}", elapsed.as_millis());
}
