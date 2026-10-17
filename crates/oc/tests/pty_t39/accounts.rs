//! Actual configless binary: secret input is masked, ACK precedes explicit choice.
use super::*;
use oc_adapters::{
    auth::{AuthScope, GO_BASE_URL},
    storage::Db,
};

const KEY: &str = "GO05_PTY_KEY_NEVER_RENDER_54c1";
const CANCELLED: &str = "GO05_CANCELLED_NEVER_SAVE_8e43";
const GO_KEY: &str = "GO05_GO_SCOPE_NOT_CUSTOM_901c";
/// Any mounted DialogSelect (integration/account list) shows its Search field.
const ACCOUNTS_SEARCH: &str = "Search";

fn paste(pty: &mut PtySession, text: &str) {
    pty.send(format!("\x1b[200~{text}\x1b[201~").as_bytes());
}

fn connect_go(pty: &mut PtySession) {
    pty.send(b"/connect\r");
    wait_dialog_title(pty, "Connect an integration");
    wait_screen_row(pty, "OpenCode Go", DEADLINE);
    pty.send(b"\r");
    // Pinned KeyMethod DialogPrompt.
    wait_dialog_title(pty, "Connect OpenCode Go");
    wait_screen_row(pty, "API key", DEADLINE);
}

fn close(pty: &mut PtySession) {
    pty.send(b"\x1b");
    dismissed(pty, ACCOUNTS_SEARCH);
    pty.send(b"/exit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
    assert!(contains(&output, ALT_LEAVE));
    assert!(!String::from_utf8_lossy(&output).contains(KEY));
    assert!(!String::from_utf8_lossy(&output).contains(CANCELLED));
    assert!(!String::from_utf8_lossy(&output).contains(GO_KEY));
}

#[test]
fn go05_actual_configless_connect_mask_cancel_ack_accounts_and_restart() {
    let fixture = Fixture::new();
    std::fs::remove_file(
        fixture
            .root
            .path()
            .join("home/config/opencode/opencode.json"),
    )
    .unwrap();
    let db = Db::open(&fixture.data_dir()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    db.set_pref(
        "public-catalog:https://models.dev/api.json:opencode-go:v1",
        &serde_json::json!({"source":"https://models.dev/api.json", "fetched_at_ms":now,
        "record":{"id":"opencode-go", "npm":"@ai-sdk/openai-compatible", "models":{
          "cached-model":{"id":"cached-model","name":"GO05 cached choice", "tool_call":true,
            "limit":{"context":32000,"output":2048}}
        }}})
        .to_string(),
    )
    .unwrap();
    drop(db);
    let mut pty = PtySession::spawn(fixture.clone(), "go05-connect", None);
    pty.wait_visible(READY, DEADLINE);
    connect_go(&mut pty);
    paste(&mut pty, CANCELLED);
    wait_screen_row(&pty, "••••", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Connect OpenCode Go");
    connect_go(&mut pty);
    paste(&mut pty, KEY);
    wait_screen_row(&pty, "••••", DEADLINE);
    pty.send(b"\r");
    // Pinned DialogModel providerID: the scoped title is the provider name.
    wait_screen_row(&pty, SCOPED_MODEL_DIALOG, DEADLINE);
    wait_screen_row(&pty, "OpenCode Go", DEADLINE);
    wait_screen_row(&pty, "GO05 cached choice", DEADLINE);
    // Dismiss without selecting: an account ACK must not select a model.
    pty.send(b"\x1b");
    dismissed(&pty, SCOPED_MODEL_DIALOG);
    pty.send(b"/accounts\r");
    // manageConnections: Add account, then the current (●) credential.
    wait_dialog_title(&pty, "OpenCode Go");
    wait_screen_row(&pty, "Connected accounts", DEADLINE);
    wait_screen_row(&pty, "● default", DEADLINE);
    pty.send(b"\x1b[B"); // Add account is the shared first row.
    pty.send(b"\x12"); // ctrl+r: dialog.integration.rename
    wait_screen_row(&pty, "Rename account", DEADLINE);
    paste(&mut pty, " renamed");
    pty.send(b"\r");
    wait_screen_row(&pty, "● default renamed", DEADLINE);
    close(&mut pty);
    let db = Db::open(&fixture.data_dir()).unwrap();
    let scope = AuthScope::admit("opencode-go", GO_BASE_URL).unwrap();
    let rows = db.credential_accounts(scope.namespace()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "default renamed");
    assert!(db.session_meta("go05-connect").unwrap().model.is_none());
    drop(db);
    let mut restarted = PtySession::spawn(fixture.clone(), "go05-connect", None);
    restarted.wait_visible(READY, DEADLINE);
    restarted.send(b"/accounts\r");
    wait_screen_row(&restarted, "● default renamed", DEADLINE);
    restarted.send(b"\x1b[B");
    restarted.send(b"\x04"); // ctrl+d: dialog.integration.delete
    wait_screen_row(&restarted, "Press ctrl+d again to confirm", DEADLINE);
    restarted.send(b"\x04");
    // Removing the final credential closes with a success toast.
    wait_screen_row(&restarted, "Disconnected OpenCode Go", DEADLINE);
    close(&mut restarted);
    let db = Db::open(&fixture.data_dir()).unwrap();
    assert!(
        db.credential_accounts(scope.namespace())
            .unwrap()
            .is_empty()
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
}

#[test]
fn go05_connect_from_custom_connection_opens_only_go_without_changing_selection() {
    let fixture = Fixture::new();
    let config_path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    // Drop the base `provider.use` deny policy for opencode-go.
    config.as_object_mut().unwrap().remove("experimental");
    std::fs::write(config_path, config.to_string()).unwrap();
    let db = Db::open(&fixture.data_dir()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    db.set_pref("public-catalog:https://models.dev/api.json:opencode-go:v1", &serde_json::json!({
        "source":"https://models.dev/api.json", "fetched_at_ms":now,
        "record":{"id":"opencode-go", "npm":"@ai-sdk/openai-compatible", "models":{
            "cached/slash":{"id":"cached/slash","name":"GO05 independent Go choice", "tool_call":true,
                "limit":{"context":32000,"output":2048}}
        }}
    }).to_string()).unwrap();
    drop(db);
    let mut pty = PtySession::spawn(fixture.clone(), "go05-custom-connect", None);
    pty.wait_visible(READY, DEADLINE);
    connect_go(&mut pty);
    paste(&mut pty, KEY);
    pty.send(b"\r");
    wait_screen_row(&pty, SCOPED_MODEL_DIALOG, DEADLINE);
    wait_screen_row(&pty, "GO05 independent Go choice", DEADLINE);
    let screen = render_screen(&pty.snapshot()).rows().join("\n");
    assert!(!screen.contains("T39 model") && !screen.contains("T39 alt"));
    pty.send(b"\x1b");
    dismissed(&pty, SCOPED_MODEL_DIALOG);
    pty.send(b"/accounts\r");
    wait_dialog_title(&pty, "fixture");
    // The connect ACK changed only Go credentials, not the current connection/model.
    pty.send(b"\x1b");
    dismissed(&pty, ACCOUNTS_SEARCH);
    pty.send(b"/connect\r");
    wait_dialog_title(&pty, "Connect an integration");
    wait_screen_row(&pty, "✓ OpenCode Go", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Add account", DEADLINE);
    pty.send(b"\r");
    wait_dialog_title(&pty, "Connect OpenCode Go");
    paste(&mut pty, KEY);
    pty.send(b"\r");
    wait_screen_row(&pty, "GO05 independent Go choice", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, SCOPED_MODEL_DIALOG);
    // Picker choice is a draft. Blank Enter is the existing explicit commit.
    pty.send(b"\r");
    // The draft label is already Go before its asynchronous commit ACK. Observe
    // durable selection, then let the ordinary idle reconciliation adopt it.
    let sql = rusqlite::Connection::open_with_flags(
        fixture.data_dir().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let deadline = Instant::now() + DEADLINE;
    while sql
        .query_row(
            "SELECT count(*) FROM prefs WHERE key LIKE 'tui.selection.session:%' \
             AND json_valid(value) \
             AND json_extract(value,'$.models.build.provider')='opencode-go' \
             AND json_extract(value,'$.models.build.id')='cached/slash'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        == 0
    {
        assert!(
            Instant::now() < deadline,
            "explicit Go selection was not committed"
        );
        std::thread::sleep(POLL);
    }
    drop(sql);
    wait_idle(&pty);
    pty.send(b"/accounts\r");
    wait_dialog_title(&pty, "OpenCode Go");
    close(&mut pty);
    let mut pty = PtySession::spawn(fixture.clone(), "go05-custom-connect", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/accounts\r");
    wait_dialog_title(&pty, "OpenCode Go");
    close(&mut pty);
    let db = Db::open(&fixture.data_dir()).unwrap();
    let scope = AuthScope::admit("opencode-go", GO_BASE_URL).unwrap();
    assert_eq!(db.credential_accounts(scope.namespace()).unwrap().len(), 2);
    assert!(
        db.session_meta("go05-custom-connect")
            .unwrap()
            .model
            .is_none()
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
}

#[test]
fn go05_configless_go_then_explicit_custom_generation_cancel_and_reopen_share_owner() {
    let fixture = Fixture::new();
    *fixture.expected_auth.lock().unwrap() = KEY.into();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config.as_object_mut().unwrap().remove("model");
    // Drop the base `provider.use` deny policy for opencode-go.
    config.as_object_mut().unwrap().remove("experimental");
    config["providers"]["fixture"]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("apiKey");
    // Begin truly configless. The local peer is admitted later by an explicit
    // config reload; it never replaces or redirects the fixed Go authority.
    std::fs::remove_file(&path).unwrap();
    let db = Db::open(&fixture.data_dir()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    db.set_pref(
        "public-catalog:https://models.dev/api.json:opencode-go:v1",
        &serde_json::json!({
            "source":"https://models.dev/api.json", "fetched_at_ms":now,
            "record":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{
                "offline-go":{"id":"offline-go","name":"GO05 offline Go selection","tool_call":true,
                    "limit":{"context":32000,"output":2048}}
            }}
        })
        .to_string(),
    )
    .unwrap();
    drop(db);
    let session = "go05-connected-generation";
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible(READY, DEADLINE);
    connect_go(&mut pty);
    paste(&mut pty, GO_KEY);
    pty.send(b"\r");
    wait_screen_row(&pty, "GO05 offline Go selection", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, SCOPED_MODEL_DIALOG);
    pty.send(b"\r");
    pty.send(b"/accounts\r");
    wait_dialog_title(&pty, "OpenCode Go");
    pty.send(b"\x1b");
    dismissed(&pty, ACCOUNTS_SEARCH);
    assert!(fixture.requests.lock().unwrap().is_empty());
    // Retain the exact Go choice as unavailable, rather than replacing it. The
    // next executable choice is an explicit custom-provider commit below.
    config["experimental"] = serde_json::json!({"policies": [
        {"action": "provider.use", "resource": "opencode-go", "effect": "deny"}
    ]});
    std::fs::write(&path, config.to_string()).unwrap();
    wait_idle(&pty);
    pty.send(b"/reload\r");
    // A newly unavailable selection uses the brief service alert, not an old
    // technical background row or an overwritten generic success toast.
    inspect_settings_code(&mut pty, "unavailable");
    {
        let sql = rusqlite::Connection::open_with_flags(
            fixture.data_dir().join("oc.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let retained: i64 = sql
            .query_row(
                "SELECT count(*) FROM prefs WHERE json_valid(value) \
                 AND json_extract(value,'$.models.build.provider')='opencode-go' \
                 AND json_extract(value,'$.models.build.id')='offline-go'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            retained > 0,
            "reload must preserve the unavailable Go choice"
        );
        let turns: i64 = sql
            .query_row("SELECT count(*) FROM turns", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            turns, 0,
            "no Go generation was accepted before custom selection"
        );
    }
    pty.send(b"/connect\r");
    wait_dialog_title(&pty, "Connect an integration");
    wait_screen_row(&pty, "fixture", DEADLINE);
    // Popular OpenAI precedes Services; Search narrows to the custom peer.
    pty.send(b"fixture");
    pty.send(b"\r");
    wait_dialog_title(&pty, "Connect fixture");
    paste(&mut pty, KEY);
    pty.send(b"\r");
    wait_screen_row(&pty, SCOPED_MODEL_DIALOG, DEADLINE);
    wait_screen_row(&pty, "T39 model", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, SCOPED_MODEL_DIALOG);
    pty.send(b"\r"); // Explicit commit, not an account ACK side effect.
    submit(&mut pty, "GO05 connected completion");
    wait_screen_row(&pty, "echo: GO05 connected completion", DEADLINE);
    fixture.wait_requests(1);
    submit(&mut pty, "vis28 held stream");
    fixture.wait_requests(2);
    wait_screen_row(&pty, "esc interrupt", DEADLINE);
    pty.send(b"\x1b");
    wait_screen_row(&pty, "esc again to interrupt", DEADLINE);
    pty.send(b"\x1b");
    vis28_wait_gone(&pty);
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    pty.send(b"/accounts\r");
    wait_screen_row(&pty, "● default", DEADLINE);
    close(&mut pty);
    let history = persisted(&fixture.data_dir(), session);
    assert!(
        history
            .iter()
            .any(|(role, text)| role == "assistant" && text == "echo: GO05 connected completion")
    );
    assert!(
        !history
            .iter()
            .any(|(_, text)| text == "answer:vis28 completed")
    );
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible("Fixture session title", DEADLINE);
    wait_screen_row(&pty, "echo: GO05 connected completion", DEADLINE);
    pty.send(b"/accounts\r");
    wait_dialog_title(&pty, "fixture");
    wait_screen_row(&pty, "● default", DEADLINE);
    close(&mut pty);
    let sql = rusqlite::Connection::open_with_flags(
        fixture.data_dir().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let raw: String = sql
        .query_row(
            "SELECT model_ref FROM turn_acceptances WHERE session_id=?1 ORDER BY rowid LIMIT 1",
            [session],
            |row| row.get(0),
        )
        .unwrap();
    let stored: oc_core::queries::ModelRef = serde_json::from_str(&raw).unwrap();
    assert_eq!(stored.provider, "fixture");
    let accounts: i64 = sql
        .query_row("SELECT count(*) FROM credential_accounts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        accounts, 2,
        "Go and custom credentials have independent scopes"
    );
    assert_eq!(fixture.wait_requests(2).len(), 2);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
}
