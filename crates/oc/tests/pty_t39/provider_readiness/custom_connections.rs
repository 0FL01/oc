//! Native-only onboarding extension, using the existing real PTY and HTTP peer.
use super::*;

const KEY: &str = "CONN04_MASKED_KEY_27f4";
const NEW_KEY: &str = "CONN04_REPLACEMENT_KEY_83a6";
const MODEL_ID: &str = "conn04/exact-model";
const NAME: &str = "CONN04 supplied model";

fn setup(fixture: &Fixture) -> (PathBuf, String, String) {
    let path = source(fixture);
    let config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let base = config["providers"]["fixture"]["settings"]["baseURL"]
        .as_str()
        .unwrap()
        .to_owned();
    // No whitelist (which would correctly refuse onboarding); all native presets
    // are explicitly disabled, without relying on a public catalog/network failure.
    let text = json!({"experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}, {"action": "provider.use", "resource": "openai", "effect": "deny"}]}}).to_string();
    std::fs::write(&path, &text).unwrap();
    *fixture.expected_auth.lock().unwrap() = KEY.into();
    *fixture.models.lock().unwrap() = json!({"object":"list","data":[{
        "id": MODEL_ID, "opencode": {"name":NAME,
            "modalities":{"input":["text","image"],"output":["text"]},
            "limit":{"context":32768,"output":4096},
            "variants":{"precise":{"reasoningEffort":"exact-effort"}}
        }
    }]});
    (path, text, base)
}

fn paste(pty: &mut PtySession, text: &str) {
    pty.send(format!("\x1b[200~{text}\x1b[201~").as_bytes());
}

fn form(pty: &mut PtySession, base: &str) {
    pty.send(b"/connect\r");
    wait_screen_row(pty, "Connect an integration", DEADLINE);
    wait_screen_row(pty, "Custom provider", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(pty, "Display name", DEADLINE);
    paste(pty, "CONN04 LAN service");
    pty.send(b"\r");
    wait_screen_row(pty, "Literal API base URL", DEADLINE);
    paste(pty, base);
    pty.send(b"\r");
    wait_screen_row(pty, "No authentication (explicit)", DEADLINE);
    pty.send(b"\r");
    dismissed(pty, "No authentication (explicit)");
    wait_screen_row(pty, "enter submit", DEADLINE);
    paste(pty, KEY);
    wait_screen_row(pty, "••••", DEADLINE);
    assert!(!String::from_utf8_lossy(&pty.snapshot()).contains(KEY));
}

fn sql(fixture: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        fixture.data_dir().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}

fn counts(fixture: &Fixture, connections: i64, accounts: i64, sessions: i64, turns: i64) {
    let db = sql(fixture);
    for (table, expected) in [
        ("custom_provider_connections", connections),
        ("credential_accounts", accounts),
        ("sessions", sessions),
        ("turns", turns),
        ("tool_operations", 0),
    ] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, expected, "{table}");
    }
}

fn saved_id(fixture: &Fixture) -> String {
    sql(fixture)
        .query_row("SELECT id FROM custom_provider_connections", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn saved_cli(fixture: &Fixture, complete: bool, expected: &str) {
    let prefs = || {
        sql(fixture)
            .prepare("SELECT key,value FROM prefs ORDER BY key")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let before = prefs();
    let gets = fixture.discoveries.load(Ordering::Relaxed);
    // The PTY still owns the writer lock. This must use the existing read-only
    // SQLite projection, not a second application or a probe/credential loader.
    let output = fixture
        .command()
        .env_remove("OC_TEST_ALLOW_LOOPBACK")
        .arg("models")
        .output()
        .unwrap();
    assert_eq!(output.status.success(), complete);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    assert_eq!(gets, fixture.discoveries.load(Ordering::Relaxed));
    assert_eq!(
        before,
        prefs(),
        "models must not write selection or other preferences"
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains(KEY) && !stderr.contains(NEW_KEY));
}

fn quit(pty: &mut PtySession) {
    pty.send(b"/exit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
    assert!(contains(&output, ALT_LEAVE));
    let output = String::from_utf8_lossy(&output);
    assert!(!output.contains(KEY) && !output.contains(NEW_KEY));
}

#[test]
fn conn04_zero_provider_native_connect_mask_cancel_ack_exact_request_and_restart() {
    let fixture = Fixture::new();
    let (path, config, base) = setup(&fixture);
    let mut pty = PtySession::spawn_custom(fixture.clone(), None);
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Sessions");
    pty.send(b"CONN04 no model yet\r");
    wait_screen_row(&pty, "model_unavailable", DEADLINE);
    counts(&fixture, 0, 0, 0, 0);
    pty.send(b"\x03"); // Clear the refused draft, not a key or a created session.
    form(&mut pty, &base);
    pty.send(b"\x1b");
    dismissed(&pty, "Connect a custom provider");
    counts(&fixture, 0, 0, 0, 0);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
    form(&mut pty, &base);
    pty.send(b"\r");
    wait_screen_row(&pty, "Confirm generation API", DEADLINE);
    wait_screen_row(&pty, "Catalog: 1 models", DEADLINE);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 1);
    assert!(fixture.requests.lock().unwrap().is_empty());
    counts(&fixture, 0, 0, 0, 0);
    pty.send(b"\r\r"); // Save once; a pending repeat must not queue another save.
    wait_screen_row(&pty, SCOPED_MODEL_DIALOG, DEADLINE);
    wait_screen_row(&pty, NAME, DEADLINE);
    counts(&fixture, 1, 1, 0, 0);
    let id = saved_id(&fixture);
    saved_cli(&fixture, true, &format!("{id}/{MODEL_ID}\n"));
    pty.send(b"\x1b");
    dismissed(&pty, SCOPED_MODEL_DIALOG);
    // Closing after the durable ACK does not undo the connection or select a model.
    counts(&fixture, 1, 1, 0, 0);
    choose_model(&mut pty, NAME);
    choose_variant(&mut pty, "precise");
    pty.send(b"\r"); // Explicit composer commit; still no fresh root or request.
    counts(&fixture, 1, 1, 0, 0);
    submit(&mut pty, "CONN04 explicit connected completion");
    wait_screen_row(&pty, "echo: CONN04 explicit connected completion", DEADLINE);
    let requests = fixture.wait_requests(1);
    assert_eq!(requests[0]["model"], MODEL_ID);
    assert_eq!(requests[0]["reasoning"]["effort"], "exact-effort");
    pty.send(b"/accounts\r");
    wait_dialog_title(&pty, "CONN04 LAN service");
    wait_screen_row(&pty, "● default", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Connected accounts");
    quit(&mut pty);
    let session: String = sql(&fixture)
        .query_row("SELECT id FROM sessions", [], |row| row.get(0))
        .unwrap();
    let mut restarted = PtySession::spawn_custom(fixture.clone(), Some(&session));
    wait_screen_row(
        &restarted,
        "echo: CONN04 explicit connected completion",
        DEADLINE,
    );
    submit(&mut restarted, "CONN04 restart exact binding");
    wait_screen_row(&restarted, "echo: CONN04 restart exact binding", DEADLINE);
    assert_eq!(fixture.wait_requests(2)[1]["model"], MODEL_ID);
    quit(&mut restarted);
    counts(&fixture, 1, 1, 1, 2);
    assert_eq!(std::fs::read_to_string(path).unwrap(), config);
    assert_eq!(
        fixture.discoveries.load(Ordering::Relaxed),
        1,
        "a ready saved catalog must not start a GET/ProviderChanged loop"
    );
}

#[test]
fn conn04_saved_failed_catalog_retry_and_key_change_join_old_get_then_empty_is_complete() {
    let fixture = Fixture::new();
    let (_, _, base) = setup(&fixture);
    fixture.catalog_control.status.store(503, Ordering::Relaxed);
    let mut pty = PtySession::spawn_custom(fixture.clone(), None);
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    form(&mut pty, &base);
    pty.send(b"\r");
    wait_screen_row(&pty, "Discovery unavailable", DEADLINE);
    counts(&fixture, 0, 0, 0, 0);
    pty.send(b"\r");
    wait_screen_row(&pty, SCOPED_MODEL_DIALOG, DEADLINE);
    wait_screen_row(&pty, "Catalog: failed", DEADLINE);
    wait_screen_row(&pty, "Retry GET", DEADLINE);
    counts(&fixture, 1, 1, 0, 0);
    saved_cli(&fixture, false, "");
    let previous = fixture.discoveries.load(Ordering::Relaxed);
    fixture.catalog_control.status.store(200, Ordering::Relaxed);
    fixture.catalog_control.hold.store(true, Ordering::Relaxed);
    pty.send(b"\x1b[D");
    wait_screen_row(&pty, "Catalog: pending", DEADLINE);
    wait_count(&fixture.discoveries, previous + 1);
    pty.send(b"\x1b[C");
    wait_screen_row(&pty, "● default", DEADLINE);
    pty.send(b"\r"); // Add account is the first row.
    wait_dialog_title(&pty, "Connect CONN04 LAN service");
    paste(&mut pty, NEW_KEY);
    *fixture.expected_auth.lock().unwrap() = NEW_KEY.into();
    pty.send(b"\r");
    wait_screen_row(&pty, SCOPED_MODEL_DIALOG, DEADLINE);
    wait_screen_row(&pty, "Catalog: pending", DEADLINE);
    wait_count(&fixture.catalog_control.closed, 1);
    wait_count(&fixture.discoveries, previous + 2);
    pty.send(b"\x1b[D"); // Same binding already pending: coalesce, do not restart.
    fixture.catalog_control.hold.store(false, Ordering::Relaxed);
    wait_screen_row(&pty, NAME, DEADLINE);
    wait_screen_row(&pty, "Catalog: ready", DEADLINE);
    let id = saved_id(&fixture);
    saved_cli(&fixture, true, &format!("{id}/{MODEL_ID}\n"));
    let (revision, catalog): (i64, i64) = sql(&fixture)
        .query_row(
            "SELECT binding_revision,catalog_revision FROM custom_provider_connections",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((revision, catalog), (2, 2));
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), previous + 2);
    *fixture.models.lock().unwrap() = json!({"object":"list","data":[]});
    pty.send(b"\x1b[D");
    wait_screen_row(&pty, "Catalog: empty", DEADLINE);
    saved_cli(&fixture, true, "");
    counts(&fixture, 1, 2, 0, 0);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x1b");
    dismissed(&pty, SCOPED_MODEL_DIALOG);
    quit(&mut pty);
}

fn wait_count(counter: &AtomicUsize, count: usize) {
    let start = Instant::now();
    while counter.load(Ordering::Relaxed) < count {
        assert!(start.elapsed() < DEADLINE, "bounded HTTP barrier {count}");
        std::thread::sleep(POLL);
    }
}
