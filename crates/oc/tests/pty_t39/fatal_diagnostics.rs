//! CFG10 native functional qualification, declared native diagnostic frames.
use super::*;
use oc_adapters::storage::Db;
use serde_json::{Value, json};

const CANARY: &str = "CFG10_NATIVE_AUTH_ENV_PATH_SECRET_1963";
pub(super) const POLICY_PROMPT: &str = "cfg10 prove retained read Deny";

fn config_path(fixture: &Fixture) -> PathBuf {
    fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json")
}

fn assert_no_remote_effects(fixture: &Fixture) {
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
    assert!(fixture.requests.lock().unwrap().is_empty());
}

fn fatal(fixture: Arc<Fixture>, code: &str, stage: &str, field: &str) {
    let previous = fixture.requests.lock().unwrap().len();
    let discovered = fixture.discoveries.load(Ordering::Relaxed);
    let headless = fixture
        .command()
        .args(["run", "--json", "cfg10 forbidden prompt"])
        .output()
        .unwrap();
    assert_eq!(headless.status.code(), Some(1));
    assert!(headless.stdout.is_empty());
    let error = String::from_utf8(headless.stderr).unwrap();
    assert!(error.contains(code), "{error}");
    assert!(error.contains(&format!(" {stage}:")), "{error}");
    assert!(error.contains(field), "{error}");
    assert!(error.contains("source-"), "{error}");
    assert!(error.contains("retryable=false"), "{error}");
    assert!(!error.contains(CANARY), "{error}");
    assert!(!error.contains("\u{1b}"), "{error}");
    let mut pty = PtySession::spawn_sized(fixture.clone(), "cfg10-fatal", None, 120, 40);
    wait_screen_row(&pty, "Native startup error", DEADLINE);
    wait_screen_row(&pty, code, DEADLINE);
    wait_screen_row(&pty, "Source: source-", DEADLINE);
    wait_screen_row(&pty, "Field:", DEADLINE);
    wait_screen_row(&pty, "retryable=false", DEADLINE);
    pty.send(b"\x1b");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert_eq!(status.code(), Some(1));
    assert!(pty.restored());
    assert!(
        output
            .windows(ALT_LEAVE.len())
            .any(|bytes| bytes == ALT_LEAVE)
    );
    assert!(!String::from_utf8_lossy(&output).contains(CANARY));
    assert_eq!(fixture.requests.lock().unwrap().len(), previous);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), discovered);
}

#[test]
fn cfg10_native_optional_definition_is_visible_but_security_reload_remains_atomic() {
    let fixture = Fixture::new();
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(config_path(&fixture)).unwrap()).unwrap();
    config["permissions"]["read"] = json!("deny");
    config["mcp"][CANARY] = Value::Null;
    std::fs::write(config_path(&fixture), config.to_string()).unwrap();
    let commands = config_path(&fixture).parent().unwrap().join("commands");
    std::fs::create_dir_all(&commands).unwrap();
    std::fs::write(
        commands.join(format!("{CANARY}.md")),
        format!("---\n{CANARY}: [\n---\nhttps://user:{CANARY}@private.invalid/{CANARY}\u{1b}[31m"),
    )
    .unwrap();
    std::fs::write(
        commands.join("healthy.md"),
        "---\ndescription: CFG10 healthy definition\n---\nhealthy payload $1",
    )
    .unwrap();
    let instructions = fixture.root.path().join("project/AGENTS.md");
    std::fs::write(&instructions, "CFG10 retained instruction generation").unwrap();
    let mut pty = PtySession::spawn_sized(fixture.clone(), "cfg10-atomic", None, 120, 40);
    wait_screen_row(&pty, READY, DEADLINE);
    wait_idle(&pty);
    assert_no_remote_effects(&fixture);
    pty.send(b"/settings\r");
    wait_screen_row(&pty, "invalid_definition", DEADLINE);
    wait_screen_row(&pty, "Configuration diagnostics", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Configuration diagnostics");
    // A complete global source does not permit dropping malformed local policy.
    let policy = fixture.root.path().join("project/opencode.jsonc");
    std::fs::write(&policy, format!("{{\"permissions\":{{\"read\":\"{CANARY}\"}},\"dcp\":{{\"enabled\":false}},\"model\":\"fixture/{CANARY}\",\"agent\":{{")).unwrap();
    std::fs::write(&instructions, "CFG10 unadmitted new instruction generation").unwrap();
    pty.send(b"/reload\r");
    wait_screen_row(&pty, "Configuration reload failed", DEADLINE);
    wait_screen_row(&pty, "invalid_document", DEADLINE);
    assert_no_remote_effects(&fixture);
    pty.send(b"\x03");
    wait_idle(&pty);
    submit(&mut pty, "cfg10 retained generation prompt");
    wait_screen_row(&pty, "echo: cfg10 retained generation prompt", DEADLINE);
    let request = fixture.wait_requests(1).remove(0);
    assert_eq!(request["model"], MODEL);
    let encoded = request.to_string();
    assert!(encoded.contains("CFG10 retained instruction generation"));
    assert!(!encoded.contains("CFG10 unadmitted new instruction generation"));
    assert!(!encoded.contains(CANARY));
    assert_eq!(
        request["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|tool| tool["name"] == "compress")
            .count(),
        1
    );
    wait_idle(&pty);
    std::fs::write(
        fixture.root.path().join("project/cfg10-denied-note.txt"),
        CANARY,
    )
    .unwrap();
    submit(&mut pty, POLICY_PROMPT);
    wait_screen_row(&pty, "answer:compressed", DEADLINE);
    let requests = fixture.wait_requests(3);
    let continuation = requests.last().unwrap();
    assert!(
        continuation["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "function_call_output"
                && item["output"]
                    .as_str()
                    .is_some_and(|text| text.contains("denied read")))
    );
    assert!(!continuation.to_string().contains(CANARY));
    wait_idle(&pty);
    pty.send(b"/exit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert_eq!(status.code(), Some(0));
    assert!(pty.restored());
    assert!(
        output
            .windows(ALT_LEAVE.len())
            .any(|bytes| bytes == ALT_LEAVE)
    );
    assert!(!String::from_utf8_lossy(&output).contains(CANARY));
    assert_eq!(
        fixture.requests.lock().unwrap().len(),
        4,
        "two prompts, denied-read continuation and one title"
    );
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    // The existing native builtin lifecycle records a policy refusal as Failed;
    // the actual function output above proves Deny, rather than dispatch/read.
    assert_eq!(
        conn.query_row(
            "SELECT state FROM tool_operations WHERE session_id = 'cfg10-atomic' AND name = 'read'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "failed"
    );
    drop(conn);
    // Reopen cannot silently discard the newly broken security source.
    fatal(fixture, "invalid_document", "config", "document");
}

#[test]
fn cfg10_native_fatal_lock_trust_policy_recovery_and_caps_are_non_success() {
    // Every case uses a fresh known-owned fake transport/data root; no paid IO.
    let fixture = Fixture::new();
    let source = fixture.root.path().join("project/opencode.jsonc");
    std::fs::write(
        &source,
        format!("{{\"permissions\":{{\"read\":\"{CANARY}\"}}}}"),
    )
    .unwrap();
    fatal(fixture, "invalid_config", "config", "permissions");

    let fixture = Fixture::new();
    let outside = fixture
        .root
        .path()
        .join(format!("{CANARY}\u{1b}[31m-outside"));
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.root.path().join("project/.opencode")).unwrap();
    fatal(fixture, "trust_refused", "admission", "root");

    let fixture = Fixture::new();
    let owner = Db::open(&fixture.data_dir()).unwrap();
    fatal(fixture.clone(), "data_root_busy", "storage", "data_root");
    drop(owner);
    // The rejected owner never corrupts or deletes the lock; later ownership works.
    drop(Db::open(&fixture.data_dir()).unwrap());

    let fixture = Fixture::new();
    let db = Db::open(&fixture.data_dir()).unwrap();
    db.create_session("cfg10-recovery").unwrap();
    drop(db);
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    conn.execute_batch(&format!("INSERT INTO tool_operations(id,session_id,name,state,input) VALUES ('cfg10-op','cfg10-recovery','read','started','{{}}'); CREATE TRIGGER cfg10_refuse_recovery BEFORE UPDATE ON tool_operations BEGIN SELECT RAISE(ABORT, '{CANARY}'); END;")).unwrap();
    drop(conn);
    fatal(fixture.clone(), "recovery_failed", "recovery", "operations");
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT state FROM tool_operations WHERE id='cfg10-op'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "started"
    );

    let fixture = Fixture::new();
    let cap = fixture.root.path().join("project/opencode.jsonc");
    std::fs::File::create(cap)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    fatal(fixture, "capacity_exceeded", "config", "document");

    let fixture = Fixture::new();
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(config_path(&fixture)).unwrap()).unwrap();
    let url = config["provider"]["fixture"]["options"]["baseURL"]
        .as_str()
        .unwrap()
        .to_string();
    config["mcp"] = Value::Object(
        (0..9)
            .map(|index| {
                (
                    format!("{CANARY}-{index}"),
                    json!({"type":"remote","url":url}),
                )
            })
            .collect(),
    );
    std::fs::write(config_path(&fixture), config.to_string()).unwrap();
    fatal(fixture, "capacity_exceeded", "admission", "mcp");

    let fixture = Fixture::new();
    let db = Db::open(&fixture.data_dir()).unwrap();
    db.set_pref(
        oc_core::queries::PREF_MODEL_SELECTION,
        &format!("{{\"id\":\"{CANARY}\""),
    )
    .unwrap();
    drop(db);
    fatal(fixture, "invalid_stored_state", "query", "selection.model");

    let fixture = Fixture::new();
    let db = Db::open(&fixture.data_dir()).unwrap();
    db.set_pref(oc_core::queries::PREF_MODEL_SELECTION, "{}")
        .unwrap();
    drop(db);
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    conn.execute(
        "UPDATE prefs SET value = ?1 WHERE key = ?2",
        rusqlite::params![CANARY.as_bytes(), oc_core::queries::PREF_MODEL_SELECTION],
    )
    .unwrap();
    drop(conn);
    fatal(fixture, "storage_unavailable", "query", "selection.model");
}
