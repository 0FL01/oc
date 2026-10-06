//! Actual configless binary: secret input is masked, ACK precedes explicit choice.
use super::*;
use oc_adapters::{
    auth::{AuthScope, GO_BASE_URL},
    storage::Db,
};

const KEY: &str = "GO05_PTY_KEY_NEVER_RENDER_54c1";
const CANCELLED: &str = "GO05_CANCELLED_NEVER_SAVE_8e43";

fn paste(pty: &mut PtySession, text: &str) {
    pty.send(format!("\x1b[200~{text}\x1b[201~").as_bytes());
}

fn close(pty: &mut PtySession) {
    pty.send(b"\x1b");
    dismissed(pty, "Connect / accounts");
    pty.send(b"/exit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
    assert!(contains(&output, ALT_LEAVE));
    assert!(!String::from_utf8_lossy(&output).contains(KEY));
    assert!(!String::from_utf8_lossy(&output).contains(CANCELLED));
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
    pty.send(b"/connect\r");
    wait_screen_row(&pty, "Account label", DEADLINE);
    paste(&mut pty, "Cancelled");
    pty.send(b"\r");
    wait_screen_row(&pty, "API key (masked)", DEADLINE);
    paste(&mut pty, CANCELLED);
    wait_screen_row(&pty, "••••", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Connect / accounts");
    pty.send(b"/connect\r");
    wait_screen_row(&pty, "Account label", DEADLINE);
    paste(&mut pty, "First account");
    pty.send(b"\r");
    wait_screen_row(&pty, "API key (masked)", DEADLINE);
    paste(&mut pty, KEY);
    wait_screen_row(&pty, "••••", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "GO05 cached choice", DEADLINE);
    // Dismiss without selecting: an account ACK must not select a model.
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    pty.send(b"/accounts\r");
    wait_screen_row(&pty, "First account [active]", DEADLINE);
    wait_screen_row(&pty, "Effective auth: Stored", DEADLINE);
    pty.send(b"r");
    wait_screen_row(&pty, "Rename account", DEADLINE);
    paste(&mut pty, " renamed");
    pty.send(b"\r");
    wait_screen_row(&pty, "First account renamed [active]", DEADLINE);
    close(&mut pty);
    let db = Db::open(&fixture.data_dir()).unwrap();
    let scope = AuthScope::admit("opencode-go", GO_BASE_URL).unwrap();
    let rows = db.credential_accounts(scope.namespace()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "First account renamed");
    assert!(db.session_meta("go05-connect").unwrap().model.is_none());
    drop(db);
    let mut restarted = PtySession::spawn(fixture.clone(), "go05-connect", None);
    restarted.wait_visible(READY, DEADLINE);
    restarted.send(b"/accounts\r");
    wait_screen_row(&restarted, "First account renamed [active]", DEADLINE);
    restarted.send(b"d");
    wait_screen_row(&restarted, "confirm removal", DEADLINE);
    restarted.send(b"d");
    wait_screen_row(&restarted, "Effective auth: Missing", DEADLINE);
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
    config.as_object_mut().unwrap().remove("disabled_providers");
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
    pty.send(b"/connect\r");
    wait_screen_row(&pty, "Account label", DEADLINE);
    paste(&mut pty, "Go from custom");
    pty.send(b"\r");
    wait_screen_row(&pty, "API key (masked)", DEADLINE);
    paste(&mut pty, KEY);
    pty.send(b"\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "GO05 independent Go choice", DEADLINE);
    let screen = render_screen(&pty.output.lock().unwrap()).rows().join("\n");
    assert!(!screen.contains("T39 model") && !screen.contains("T39 alt"));
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    pty.send(b"/accounts\r");
    wait_screen_row(&pty, "Provider: fixture", DEADLINE);
    // The connect ACK changed only Go credentials, not the current connection/model.
    close(&mut pty);
    let db = Db::open(&fixture.data_dir()).unwrap();
    let scope = AuthScope::admit("opencode-go", GO_BASE_URL).unwrap();
    assert_eq!(db.credential_accounts(scope.namespace()).unwrap().len(), 1);
    assert!(
        db.session_meta("go05-custom-connect")
            .unwrap()
            .model
            .is_none()
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
}
