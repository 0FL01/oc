//! SIGKILL of the owning process at each durable point of a tool effect —
//! after its acknowledged intent, after the effect, after its outcome. The
//! test binary re-runs this test as the killed child.
use super::*;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const CHILD: &str = "OC_FAULT_TEST_CHILD";
const TEST: &str = "storage::fault_tests::kill_at_each_durable_point_never_replays_the_effect";

/// Child: each point is acknowledged on stdout, then the child waits on stdin
/// until it is killed.
fn child(root: &Path) -> ! {
    let db = Db::open(&root.join("data")).unwrap();
    db.create_session("s").unwrap();
    let mut stdin = std::io::stdin().lock();
    let mut reached = |point: &str| {
        println!("{point}");
        std::io::stdout().flush().unwrap();
        stdin.read_line(&mut String::new()).unwrap();
    };
    db.record_tool_intent("effect", "s", None, "bash", "{}")
        .unwrap();
    reached("intent");
    let mut marker = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("effects"))
        .unwrap();
    marker.write_all(b"x").unwrap();
    marker.sync_all().unwrap();
    reached("effect");
    db.record_tool_outcome("effect", "completed", Some("done"))
        .unwrap();
    reached("outcome");
    std::process::exit(0);
}

#[test]
fn kill_at_each_durable_point_never_replays_the_effect() {
    if let Some(root) = std::env::var_os(CHILD) {
        child(Path::new(&root));
    }
    for (point, state, effects, recovered) in [
        ("intent", "unknown", 0, 1),
        ("effect", "unknown", 1, 1),
        ("outcome", "completed", 1, 0),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut advance = process.stdin.take().unwrap();
        let mut lines = BufReader::new(process.stdout.take().unwrap()).lines();
        loop {
            let line = lines.next().expect("child exited early").unwrap();
            // libtest's unterminated "test … ... " prefix shares the first line.
            if line.ends_with(point) {
                break;
            }
            if ["intent", "effect"]
                .iter()
                .any(|earlier| line.ends_with(earlier))
            {
                advance.write_all(b"\n").unwrap();
            }
        }
        process.kill().unwrap();
        process.wait().unwrap();
        let db = Db::open(&dir.path().join("data")).unwrap();
        let count = || fs::read(dir.path().join("effects")).map_or(0, |bytes| bytes.len());
        assert_eq!(
            db.recover_interrupted_tools().unwrap(),
            recovered,
            "{point}"
        );
        assert_eq!(db.tool_state("effect").unwrap(), state, "{point}");
        assert_eq!(count(), effects, "{point}");
        // Recovery is final: a second pass and a reopen change nothing.
        assert_eq!(db.recover_interrupted_tools().unwrap(), 0, "{point}");
        drop(db);
        let db = Db::open(&dir.path().join("data")).unwrap();
        assert_eq!(db.recover_interrupted_tools().unwrap(), 0, "{point}");
        assert_eq!(db.tool_state("effect").unwrap(), state, "{point}");
        assert_eq!(count(), effects, "{point}");
    }
}
