use super::*;
use serde_json::{Value, json};
use std::{fs, thread};

#[test]
fn vis12_actual_user_shell_records_before_effect_without_provider_or_restart_replay() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    let config = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["permissions"] = json!({"compress":"allow", "bash":"allow"});
    fs::write(config, value.to_string()).unwrap();
    let command = "python3 shell-history-probe.py";
    fs::write(project.join("shell-history-probe.py"), format!(r#"
import json, pathlib, sqlite3, time
database = pathlib.Path({database:?})
connection = sqlite3.connect(database.as_uri() + '?mode=ro', uri=True)
history = json.loads(connection.execute("SELECT value FROM prefs WHERE key='tui.prompt_history.v1'").fetchone()[0])
assert history == [{command:?}], history
rows = connection.execute("SELECT id,turn_id,state,input FROM tool_operations WHERE name='shell' ORDER BY rowid").fetchall()
assert rows[-1][1] is None and rows[-1][2] == 'started', rows
assert json.loads(rows[-1][3])['command'] == {command:?}
assert connection.execute('SELECT COUNT(*) FROM turns').fetchone()[0] == 0
with pathlib.Path('user-shell-effects.txt').open('a') as effect:
    effect.write('effect\n')
print('USER-SHELL-HISTORY-COMMITTED', flush=True)
"#, database=fixture.data_dir().join("oc.sqlite").display().to_string())).unwrap();
    seed_session(&fixture.data_dir(), &project, "user-shell-origin", 0);
    let mut pty = PtySession::spawn(fixture.clone(), "user-shell-origin", None);
    wait_screen_row(&pty, "Build", DEADLINE);
    pty.send(b"\x18n");
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    pty.send(b"!");
    wait_screen_row(&pty, "Run a command", DEADLINE);
    let discoveries = fixture.discoveries.load(Ordering::SeqCst);
    pty.send(format!("\x1b[200~{command}\x1b[201~\r").as_bytes());
    let effect = project.join("user-shell-effects.txt");
    let wait_effects = |pty: &PtySession, count| {
        let started = Instant::now();
        loop {
            if fs::read_to_string(&effect)
                .unwrap_or_default()
                .lines()
                .count()
                == count
            {
                break;
            }
            assert!(
                started.elapsed() < DEADLINE,
                "explicit Shell effect did not reach {count}: {:?}",
                render_screen(&pty.snapshot()).rows()
            );
            thread::sleep(POLL);
        }
    };
    wait_effects(&pty, 1);
    wait_screen_row(&pty, "USER-SHELL-HISTORY-COMMITTED", DEADLINE);
    wait_idle(&pty);
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "no Responses or title request for user Shell"
    );
    assert_eq!(fixture.discoveries.load(Ordering::SeqCst), discoveries);
    let connection = rusqlite::Connection::open_with_flags(
        fixture.data_dir().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let fresh: String = connection
        .query_row(
            "SELECT session_id FROM tool_operations WHERE name='shell' ORDER BY rowid LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(fresh, "user-shell-origin", "Home Shell adopts a fresh root");
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(connection);

    // Recall is text only. A second explicit ! + Enter is a new command
    // admission, not a replay of the first operation.
    pty.send(b"\x1b[A");
    wait_screen_row(&pty, command, DEADLINE);
    pty.send(b"!");
    wait_screen_row(&pty, "Shell", DEADLINE);
    pty.send(b"\r");
    wait_effects(&pty, 2);
    wait_idle(&pty);
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(fixture.discoveries.load(Ordering::SeqCst), discoveries);
    pty.send(b"\x03");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());

    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        db.get_pref("tui.prompt_history.v1")
            .unwrap()
            .map(|s| serde_json::from_str::<Vec<String>>(&s).unwrap())
            .unwrap(),
        [command]
    );
    let operations = db.list_tool_ops(&fresh).unwrap();
    assert_eq!(operations.len(), 2);
    assert!(
        operations
            .iter()
            .all(|op| op.turn.is_none() && op.state == "completed")
    );
    drop(db);
    let mut restarted = PtySession::spawn(fixture.clone(), &fresh, None);
    wait_screen_row(&restarted, "Build", DEADLINE);
    assert_eq!(
        fs::read_to_string(&effect).unwrap().lines().count(),
        2,
        "restart never relaunches prior jobs"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    restarted.send(b"\x03");
    let (status, _) = restarted.wait_exit(DEADLINE);
    assert!(status.success() && restarted.restored());
    assert_eq!(fs::read_to_string(&effect).unwrap().lines().count(), 2);
}

#[test]
fn vis12_actual_user_shell_ask_and_deny_keep_history_and_effects_at_admission_boundary() {
    for decision in ["allow-once", "reject", "deny"] {
        let fixture = Fixture::new();
        let project = fixture.root.path().join("project");
        let config = fixture
            .root
            .path()
            .join("home/config/opencode/opencode.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["permissions"] =
            json!({"compress":"allow", "bash":if decision == "deny" {"deny"} else {"ask"}});
        fs::write(config, value.to_string()).unwrap();
        seed_session(&fixture.data_dir(), &project, "user-shell-policy-origin", 0);
        let mut pty = PtySession::spawn(fixture.clone(), "user-shell-policy-origin", None);
        wait_screen_row(&pty, "Build", DEADLINE);
        pty.send(b"\x18n");
        wait_screen_row(&pty, "Ask anything", DEADLINE);
        pty.send(b"!");
        wait_screen_row(&pty, "Run a command", DEADLINE);
        let discoveries = fixture.discoveries.load(Ordering::SeqCst);
        let command = "printf OWNED-SHELL-EFFECT > policy-effect.txt";
        pty.send(format!("\x1b[200~{command}\x1b[201~\r").as_bytes());
        if decision != "deny" {
            wait_screen_row(&pty, "Permission required", DEADLINE);
            let sql = rusqlite::Connection::open_with_flags(
                fixture.data_dir().join("oc.sqlite"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            assert_eq!(
                sql.query_row("SELECT COUNT(*) FROM tool_operations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                sql.query_row(
                    "SELECT COUNT(*) FROM prefs WHERE key='tui.prompt_history.v1'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            assert!(!project.join("policy-effect.txt").exists());
            drop(sql);
            pty.send(if decision == "allow-once" {
                b"\r"
            } else {
                b"\x1b"
            });
        }
        let started = Instant::now();
        loop {
            let rows = render_screen(&pty.snapshot()).rows();
            if rows.iter().any(|r| r.contains("Reject permission")) {
                pty.send(b"\r");
            }
            let done = if decision == "allow-once" {
                project.join("policy-effect.txt").exists()
                    && rows.iter().any(|r| r.contains("Build ·"))
            } else {
                rows.iter().any(|r| r.contains("trust_refused"))
            };
            if done {
                break;
            }
            assert!(
                started.elapsed() < DEADLINE,
                "{decision} failed to settle: {rows:?}"
            );
            thread::sleep(POLL);
        }
        assert!(fixture.requests.lock().unwrap().is_empty());
        assert_eq!(fixture.discoveries.load(Ordering::SeqCst), discoveries);
        // Quit explicitly, without clearing a refused command into history.
        pty.send(b"\x10Exit the app\r");
        let (status, _) = pty.wait_exit(DEADLINE);
        assert!(status.success() && pty.restored());
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        let history = db
            .get_pref("tui.prompt_history.v1")
            .unwrap()
            .map(|raw| serde_json::from_str::<Vec<String>>(&raw).unwrap())
            .unwrap_or_default();
        if decision == "allow-once" {
            assert_eq!(history, [command]);
            assert_eq!(
                fs::read_to_string(project.join("policy-effect.txt")).unwrap(),
                "OWNED-SHELL-EFFECT"
            );
        } else {
            assert!(history.is_empty() && !project.join("policy-effect.txt").exists());
            let sql = rusqlite::Connection::open_with_flags(
                fixture.data_dir().join("oc.sqlite"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            assert_eq!(
                sql.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1,
                "refusal cannot adopt a fresh root"
            );
            assert_eq!(
                sql.query_row("SELECT COUNT(*) FROM tool_operations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}
