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
    value["permissions"] = json!([
        {"action": "compress", "resource": "*", "effect": "allow"},
        {"action": "shell", "resource": "*", "effect": "allow"}
    ]);
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
fn vis12_actual_user_shell_ignores_model_policy_and_keeps_durable_history() {
    use rusqlite::OptionalExtension;

    for permission in ["allow", "ask", "deny"] {
        let fixture = Fixture::new();
        let project = fixture.root.path().join("project");
        let config = fixture
            .root
            .path()
            .join("home/config/opencode/opencode.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["permissions"] = json!([
            {"action": "compress", "resource": "*", "effect": "allow"},
            {"action": "shell", "resource": "*", "effect": permission}
        ]);
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
        let sql = rusqlite::Connection::open_with_flags(
            fixture.data_dir().join("oc.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let started = Instant::now();
        loop {
            let rows = render_screen(&pty.snapshot()).rows();
            assert!(
                !rows.iter().any(|r| r.contains("Permission required")),
                "explicit user command must not ask: {rows:?}"
            );
            let state: Option<String> = sql
                .query_row(
                    "SELECT state FROM tool_operations WHERE name='shell' AND turn_id IS NULL ORDER BY rowid DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .unwrap();
            assert!(
                state
                    .as_deref()
                    .is_none_or(|state| matches!(state, "started" | "completed")),
                "{permission} user shell reached {state:?}"
            );
            // The effect/footer precede terminal persistence. Last-client exit
            // cancels an unfinished interactive job, so await its real outcome.
            let done = state.as_deref() == Some("completed")
                && project.join("policy-effect.txt").exists()
                && rows.iter().any(|r| r.contains("Build ·"));
            if done {
                break;
            }
            assert!(
                started.elapsed() < DEADLINE,
                "{permission} failed to settle: {rows:?}"
            );
            thread::sleep(POLL);
        }
        drop(sql);
        assert!(fixture.requests.lock().unwrap().is_empty());
        assert_eq!(fixture.discoveries.load(Ordering::SeqCst), discoveries);
        // Quit explicitly, without another input entering prompt history.
        pty.send(b"\x10Exit the app\r");
        let (status, _) = pty.wait_exit(DEADLINE);
        assert!(status.success() && pty.restored());
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        let history = db
            .get_pref("tui.prompt_history.v1")
            .unwrap()
            .map(|raw| serde_json::from_str::<Vec<String>>(&raw).unwrap())
            .unwrap_or_default();
        assert_eq!(history, [command]);
        assert_eq!(
            fs::read_to_string(project.join("policy-effect.txt")).unwrap(),
            "OWNED-SHELL-EFFECT"
        );
        let sql = rusqlite::Connection::open_with_flags(
            fixture.data_dir().join("oc.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let fresh: String = sql
            .query_row(
                "SELECT session_id FROM tool_operations WHERE name='shell' ORDER BY rowid LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(fresh, "user-shell-policy-origin");
        assert_eq!(
            sql.query_row("SELECT COUNT(*) FROM sessions", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            sql.query_row("SELECT COUNT(*) FROM turns", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(sql);
        let operations = db.list_tool_ops(&fresh).unwrap();
        assert_eq!(operations.len(), 1);
        assert!(operations[0].turn.is_none());
        assert_eq!(operations[0].state, "completed");
    }
}
