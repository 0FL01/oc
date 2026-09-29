//! UI07 actual native binary: local history/picker, typed refusal, explicit repair.
use super::*;
use oc_core::domain::SessionId;
use oc_core::queries::{ProviderStatus, ServiceCode};
use oc_core::session::Role;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CANARY: &str = "UI07_NEVER_RENDER_CONFIG_ENV_AUTH_62c4";
const DYNAMIC: &str = "ui07-arbitrary-native-id-62c4";

/// Owned fake GET barriers never stall the listener or a healthy Responses route.
pub(super) struct CatalogControl {
    status: AtomicUsize,
    hold: AtomicBool,
    closed: AtomicUsize,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

impl Default for CatalogControl {
    fn default() -> Self {
        Self {
            status: AtomicUsize::new(200),
            hold: AtomicBool::new(false),
            closed: AtomicUsize::new(0),
            threads: Mutex::new(Vec::new()),
        }
    }
}

impl CatalogControl {
    pub(super) fn reply(
        self: &Arc<Self>,
        mut socket: TcpStream,
        models: Arc<Mutex<Value>>,
        stop: Arc<AtomicBool>,
    ) {
        if self.hold.load(Ordering::Relaxed) {
            let control = self.clone();
            self.threads
                .lock()
                .unwrap()
                .push(std::thread::spawn(move || {
                    socket.set_read_timeout(Some(POLL)).unwrap();
                    while control.hold.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                        match socket.read(&mut [0]) {
                            Ok(0) => {
                                control.closed.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                            Ok(_) => panic!("unexpected bytes after a bounded GET"),
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) => {}
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::ConnectionReset
                                        | std::io::ErrorKind::ConnectionAborted
                                ) =>
                            {
                                control.closed.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                            Err(error) => panic!("held fake GET: {error}"),
                        }
                    }
                    if !stop.load(Ordering::Relaxed) {
                        control.write(&mut socket, &models);
                    }
                }));
        } else {
            self.write(&mut socket, &models);
        }
    }

    fn write(&self, socket: &mut TcpStream, models: &Mutex<Value>) {
        let status = self.status.load(Ordering::Relaxed);
        let body = if status == 200 {
            models.lock().unwrap().to_string()
        } else {
            format!("auth=https://user:{CANARY}@private.invalid/{CANARY}\u{1b}[31m\u{7}")
        };
        let result = write!(socket, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).and_then(|()| socket.flush());
        if let Err(error) = result {
            assert!(
                matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                ),
                "fake GET reply: {error}"
            );
            self.closed.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn join(&self) {
        let threads: Vec<_> = self.threads.lock().unwrap().drain(..).collect();
        for thread in threads {
            if !std::thread::panicking() {
                thread.join().expect("owned GET connection");
            }
        }
    }
}

fn source(fixture: &Fixture) -> PathBuf {
    fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json")
}

fn native_config(fixture: &Fixture) -> Value {
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(source(fixture)).unwrap()).unwrap();
    let provider = config["provider"]
        .as_object_mut()
        .unwrap()
        .remove("fixture")
        .unwrap();
    config["provider"]["ludka2"] = provider;
    config["model"] = format!("ludka2/{MODEL}").into();
    config.as_object_mut().unwrap().remove("agent");
    config["plugin"] = json!([
        "@tarquinen/opencode-dcp",
        format!("https://user:{CANARY}@private.invalid/{CANARY}\u{1b}[31m")
    ]);
    config["provider"]["ludka2"]["options"]["headers"] = json!({"x-ui07-secret":CANARY});
    config["provider"]["ludka2"]["models"][MODEL]["name"] = "UI07 stored model".into();
    config["provider"]["ludka2"]["models"][ALT_MODEL]["name"] = "UI07 healthy alternative".into();
    config
}

fn publish(fixture: &Fixture, config: &Value) {
    std::fs::write(source(fixture), config.to_string()).unwrap();
}

fn rows(fixture: &Fixture, dynamic: bool) {
    let mut data = vec![
        json!({"id":MODEL,"opencode":{"name":"UI07 stored model","limit":{"context":32768,"output":4096}}}),
        json!({"id":ALT_MODEL,"opencode":{"name":"UI07 healthy alternative","limit":{"context":32768,"output":4096}}}),
    ];
    if dynamic {
        data.push(json!({"id":DYNAMIC,"opencode":{"name":"UI07 recovered remote","limit":{"context":32768,"output":4096}}}));
    }
    *fixture.models.lock().unwrap() = json!({"object":"list","data":data});
}

fn settings(pty: &mut PtySession, status: &str) {
    wait_idle(pty);
    pty.send(b"/settings\r");
    wait_screen_row(pty, "Settings", DEADLINE);
    wait_screen_row(pty, &format!("Provider request — {status}"), DEADLINE);
    wait_screen_row(pty, "DCP — active", DEADLINE);
}

fn dismiss(pty: &mut PtySession) {
    pty.send(b"\x1b");
    dismissed(pty, "Compiled plugins");
    wait_idle(pty);
}

fn choose(pty: &mut PtySession, label: &str) {
    pty.send(b"/model\r");
    wait_screen_row(pty, label, DEADLINE);
    pty.send(label.as_bytes());
    pty.send(b"\r");
    dismissed(pty, "Select model");
    wait_idle(pty);
    if label == "UI07 healthy alternative" {
        wait_screen_row(pty, "Select variant", DEADLINE);
        pty.send(b"\r");
        dismissed(pty, "Select variant");
        wait_idle(pty);
    }
}

fn clean_exit(pty: &mut PtySession) {
    pty.send(b"/exit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "{status}");
    assert!(pty.restored());
    assert!(contains(&output, ALT_LEAVE));
    assert!(!String::from_utf8_lossy(&output).contains(CANARY));
}

fn main_count(fixture: &Fixture) -> usize {
    fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| !title::is_title(request))
        .count()
}

fn assert_wire_counts(fixture: &Fixture, main: usize, titles: usize) {
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| !title::is_title(request))
            .count(),
        main
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| title::is_title(request))
            .count(),
        titles
    );
    for request in requests.iter() {
        let body = request.to_string();
        assert!(
            !body.contains(CANARY),
            "config/remote canary entered generation context"
        );
        assert!(
            !body.contains("fixture-not-a-secret"),
            "environment credential entered context"
        );
    }
}

fn spawn(fixture: Arc<Fixture>, session: &str) -> PtySession {
    let pty = PtySession::spawn_sized(fixture, session, None, 120, 40);
    pty.wait_visible("Native runtime", DEADLINE);
    pty
}

#[test]
fn ui07_native_missing_key_preserves_history_picker_and_repairs_exact_selection() {
    let fixture = Fixture::new();
    let mut config = native_config(&fixture);
    rows(&fixture, false);
    publish(&fixture, &config);
    let mut initial = spawn(fixture.clone(), "ui07-history");
    settings(&mut initial, "ready");
    dismiss(&mut initial);
    choose(&mut initial, "UI07 healthy alternative");
    submit(&mut initial, "ui07 durable local history");
    wait_screen_row(&initial, "echo: ui07 durable local history", DEADLINE);
    assert_eq!(fixture.wait_requests(1)[0]["model"], ALT_MODEL);
    clean_exit(&mut initial);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 1);
    config["provider"]["ludka2"]["options"]["apiKey"] = "{env:UI07_ABSENT_KEY}".into();
    publish(&fixture, &config);
    let mut pty = spawn(fixture.clone(), "ui07-history");
    wait_screen_row(&pty, "echo: ui07 durable local history", DEADLINE);
    settings(&mut pty, "unavailable");
    dismiss(&mut pty);
    pty.send(b"ui07 retained refused draft\r");
    wait_screen_row(&pty, "submit: provider", DEADLINE);
    wait_screen_row(&pty, "missing_credential", DEADLINE);
    wait_screen_row(&pty, "ui07 retained refused draft", DEADLINE);
    assert_eq!(main_count(&fixture), 1);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 1);
    pty.send(b"\x03");
    config["provider"]["ludka2"]["options"]["apiKey"] = "{env:OC_FIXTURE_KEY}".into();
    publish(&fixture, &config);
    pty.send(b"/reload\r");
    wait_screen_row(&pty, "Configuration reloaded", DEADLINE);
    settings(&mut pty, "ready");
    dismiss(&mut pty);
    submit(&mut pty, "ui07 exact recovered selection");
    wait_screen_row(&pty, "echo: ui07 exact recovered selection", DEADLINE);
    assert_eq!(fixture.wait_requests(2)[1]["model"], ALT_MODEL);
    clean_exit(&mut pty);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 2);
    config["provider"]["ludka2"]["options"]["apiKey"] = "{env:UI07_ABSENT_KEY}".into();
    publish(&fixture, &config);
    let output = fixture
        .command()
        .args(["run", "--session", "ui07-history", "ui07 headless refusal"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("missing_credential") && stderr.contains("source-"));
    assert!(!stderr.contains(CANARY));
    assert_eq!(main_count(&fixture), 2);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 2);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let env = BTreeMap::from([(
                "OPENCODE_CONFIG_DIR".into(),
                fixture
                    .root
                    .path()
                    .join("home/config/opencode")
                    .to_string_lossy()
                    .into_owned(),
            )]);
            let (app, guard, _) = oc_adapters::application::spawn_with_env(
                &fixture.root.path().join("project"),
                &fixture.data_dir(),
                env,
            )
            .await
            .unwrap();
            let catalog = app
                .session_selection(
                    SessionId::new("ui07-history").unwrap(),
                    false,
                    oc_core::queries::SessionSelectionAction::Current,
                )
                .await
                .unwrap();
            assert_eq!(catalog.model_id, ALT_MODEL);
            let provider = catalog.chrome.provider.unwrap();
            assert_eq!(provider.status, ProviderStatus::Unavailable);
            assert_eq!(
                provider.diagnostic.as_ref().unwrap().code,
                ServiceCode::MissingCredential
            );
            assert!(!format!("{provider:?}").contains(CANARY));
            let page = app
                .history_page(SessionId::new("ui07-history").unwrap(), None, None, 20)
                .await
                .unwrap();
            assert_eq!(
                page.rows
                    .iter()
                    .filter(|row| row.role == Role::User)
                    .count(),
                2
            );
            assert!(!format!("{page:?}").contains("retained refused draft"));
            // Availability guards apply to requests, not a purely local history fork.
            let boundary = page
                .rows
                .iter()
                .rfind(|row| row.role == Role::User)
                .unwrap();
            let fork = app
                .fork_session(SessionId::new("ui07-history").unwrap(), boundary.id.clone())
                .await
                .unwrap();
            assert_eq!(fork.prompt, "ui07 exact recovered selection");
            let fork_catalog = app
                .session_selection(
                    fork.session.clone(),
                    false,
                    oc_core::queries::SessionSelectionAction::Current,
                )
                .await
                .unwrap();
            assert_eq!(fork_catalog.model_id, ALT_MODEL);
            assert_eq!(
                fork_catalog.chrome.provider.unwrap().status,
                ProviderStatus::Unavailable
            );
            assert_eq!(
                app.history_page(fork.session, None, None, 20)
                    .await
                    .unwrap()
                    .rows
                    .iter()
                    .filter(|row| row.role == Role::User)
                    .count(),
                1
            );
            assert_eq!(
                app.history_page(SessionId::new("ui07-history").unwrap(), None, None, 20)
                    .await
                    .unwrap(),
                page
            );
            app.shutdown().await.unwrap();
            guard.join().await.unwrap();
        });
    assert_wire_counts(&fixture, 2, 1);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 2);
    // Headless resumes the explicit session choice, not the unavailable config
    // default. This is one actual request, never a silent fallback selection.
    config["provider"]["ludka2"]["options"]["apiKey"] = "{env:OC_FIXTURE_KEY}".into();
    config["model"] = format!("ludka2/{DYNAMIC}").into();
    publish(&fixture, &config);
    let output = fixture
        .command()
        .args([
            "run",
            "--session",
            "ui07-history",
            "ui07 scoped headless exact selection",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "headless ignored an admitted stored session selection"
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("echo: ui07 scoped headless exact selection")
    );
    assert_eq!(fixture.wait_requests(3)[2]["model"], ALT_MODEL);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 3);
    assert_wire_counts(&fixture, 3, 1);
}

#[test]
fn ui07_native_auth_failures_are_pre_effect_and_failed_refresh_keeps_catalog() {
    let fixture = Fixture::new();
    let mut config = native_config(&fixture);
    rows(&fixture, true);
    fixture.catalog_control.status.store(401, Ordering::Relaxed);
    publish(&fixture, &config);
    let mut pty = spawn(fixture.clone(), "ui07-auth");
    settings(&mut pty, "failed");
    dismiss(&mut pty);
    pty.send(b"ui07 auth must not be accepted\r");
    wait_screen_row(&pty, "submit: provider", DEADLINE);
    wait_screen_row(&pty, "unauthorized", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03");
    fixture.catalog_control.status.store(200, Ordering::Relaxed);
    config["model"] = format!("ludka2/{DYNAMIC}").into();
    publish(&fixture, &config);
    pty.send(b"/reload\r");
    wait_screen_row(&pty, "Configuration reloaded", DEADLINE);
    choose(&mut pty, "UI07 recovered remote");
    submit(&mut pty, "ui07 dynamic exact first wire");
    wait_screen_row(&pty, "echo: ui07 dynamic exact first wire", DEADLINE);
    assert_eq!(fixture.wait_requests(1)[0]["model"], DYNAMIC);
    wait_idle(&pty);
    fixture.catalog_control.status.store(403, Ordering::Relaxed);
    // The effective wire binding is identical despite a trailing slash and an
    // overridden reserved header; its auth rejection applies to current.
    let original_url = config["provider"]["ludka2"]["options"]["baseURL"]
        .as_str()
        .unwrap()
        .to_owned();
    config["provider"]["ludka2"]["options"]["baseURL"] = format!("{original_url}/").into();
    config["provider"]["ludka2"]["options"]["headers"]["Authorization"] = CANARY.into();
    publish(&fixture, &config);
    pty.send(b"/reload\r");
    // ProviderChanged can replace a short-lived reload toast with the persistent
    // owner diagnostic. Assert that fact and the queried read-only status.
    wait_screen_row(&pty, "forbidden", DEADLINE);
    pty.send(b"\x03"); // Failed slash reload intentionally retains its draft.
    settings(&mut pty, "failed");
    dismiss(&mut pty);
    pty.send(b"/model\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "UI07 recovered remote", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    pty.send(b"ui07 refused after failed refresh\r");
    wait_screen_row(&pty, "submit: provider", DEADLINE);
    wait_screen_row(&pty, "forbidden", DEADLINE);
    wait_screen_row(&pty, "ui07 refused after failed refresh", DEADLINE);
    assert_eq!(main_count(&fixture), 1);
    pty.send(b"\x03");
    fixture.catalog_control.status.store(200, Ordering::Relaxed);
    config["provider"]["ludka2"]["options"]["apiKey"] = "{env:OC_FIXTURE_KEY}".into();
    config["provider"]["ludka2"]["options"]["headers"]
        .as_object_mut()
        .unwrap()
        .remove("Authorization");
    publish(&fixture, &config);
    pty.send(b"/reload\r");
    wait_screen_row(&pty, "Configuration reloaded", DEADLINE);
    settings(&mut pty, "ready");
    dismiss(&mut pty);
    submit(&mut pty, "ui07 dynamic exact next wire");
    wait_screen_row(&pty, "echo: ui07 dynamic exact next wire", DEADLINE);
    assert_eq!(fixture.wait_requests(2)[1]["model"], DYNAMIC);
    clean_exit(&mut pty);
    fixture.catalog_control.status.store(401, Ordering::Relaxed);
    let output = fixture
        .command()
        .args([
            "run",
            "--session",
            "ui07-auth",
            "ui07 headless auth refusal",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success() && output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unauthorized") && !stderr.contains(CANARY));
    assert_eq!(main_count(&fixture), 2);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 5);
    assert_wire_counts(&fixture, 2, 1);
}

#[test]
fn ui07_native_discovery_deadline_is_local_and_explicit_known_model_works() {
    let fixture = Fixture::new();
    let mut config = native_config(&fixture);
    config["model"] = format!("ludka2/{DYNAMIC}").into();
    fixture.catalog_control.hold.store(true, Ordering::Relaxed);
    publish(&fixture, &config);
    let mut pty = spawn(fixture.clone(), "ui07-deadline");
    settings(&mut pty, "pending");
    dismiss(&mut pty);
    pty.send(b"ui07 pending retained draft\r");
    wait_screen_row(&pty, "submit: provider", DEADLINE);
    wait_screen_row(&pty, "provider_pending", DEADLINE);
    wait_screen_row(&pty, "ui07 pending retained draft", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03");
    pty.send(b"/settings\r");
    // Exercise the actual unchanged 30s discovery budget; ordinary PTY deadlines
    // remain unchanged. The application must paint and accept local input now.
    wait_screen_row(
        &pty,
        "Provider request — failed",
        Duration::from_millis(oc_adapters::discovery::TOTAL_TIMEOUT_MS + 5_000),
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 2);
    dismiss(&mut pty);
    choose(&mut pty, "UI07 healthy alternative");
    submit(&mut pty, "ui07 alternative exact wire");
    wait_screen_row(&pty, "echo: ui07 alternative exact wire", DEADLINE);
    assert_eq!(fixture.wait_requests(1)[0]["model"], ALT_MODEL);
    clean_exit(&mut pty);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 2);
    assert_eq!(fixture.catalog_control.closed.load(Ordering::Relaxed), 2);
    assert_wire_counts(&fixture, 1, 1);
}

#[test]
fn ui07_native_refused_connection_keeps_picker_alive_without_generation() {
    let fixture = Fixture::new();
    let mut config = native_config(&fixture);
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = closed.local_addr().unwrap();
    drop(closed);
    config["provider"]["ludka2"]["options"]["baseURL"] = format!("http://{addr}/proxy/v1").into();
    config["model"] =
        format!("ludka2/https://user:{CANARY}@private.invalid/{CANARY}?token={CANARY}\x1b[31m")
            .into();
    publish(&fixture, &config);
    let mut pty = spawn(fixture.clone(), "ui07-connect");
    settings(&mut pty, "pending");
    dismiss(&mut pty);
    pty.send(b"/model\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "UI07 healthy alternative", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    pty.send(b"/settings\r");
    wait_screen_row(
        &pty,
        "Provider request — failed",
        Duration::from_millis(oc_adapters::discovery::TOTAL_TIMEOUT_MS + 5_000),
    );
    dismiss(&mut pty);
    pty.send(b"ui07 disconnected refusal\r");
    wait_screen_row(&pty, "connection_failed", DEADLINE);
    wait_screen_row(&pty, "ui07 disconnected refusal", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03");
    clean_exit(&mut pty);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
    assert_wire_counts(&fixture, 0, 0);
}
