//! CFG09 actual-binary native aliases + unsupported requests, reload and reopen.
use super::*;
use oc_core::queries::{PluginStatus, ServiceCode};
use serde_json::json;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;

const CANARY: &str = "CFG09_NEVER_RENDER_AUTH_PATH_ENV_309a";

fn spawn(fixture: Arc<Fixture>, trap: &Path) -> PtySession {
    let (master, slave) = openpty_pair(120, 40);
    let mut cmd = fixture.command();
    cmd.arg("tui")
        .args(["--session", "cfg09-native"])
        .env("TERM", "xterm-256color")
        .env("PATH", trap)
        .env("OC_CFG09_EXEC", fixture.root.path().join("loader-executed"))
        .env("OC_CFG09_SECRET", CANARY)
        .stdin(Stdio::from(dup_fd(&slave)))
        .stdout(Stdio::from(dup_fd(&slave)))
        .stderr(Stdio::from(dup_fd(&slave)));
    terminal::controlling_terminal(&mut cmd);
    let child = cmd.spawn().unwrap();
    drop(slave);
    PtySession::finish_spawn(master, child, fixture)
}

fn settings(pty: &mut PtySession, status: &str) {
    pty.send(b"/settings\r");
    wait_screen_row(pty, "Settings", DEADLINE);
    wait_screen_row(pty, status, DEADLINE);
}

#[test]
fn cfg09_native_mixed_plugins_reload_reopen_and_effects_are_truthful() {
    let fixture = Fixture::new();
    let global = fixture.root.path().join("home/config/opencode");
    let project = fixture.root.path().join("project");
    let source = global.join("opencode.json");
    let trap = fixture.root.path().join("trap-bin");
    std::fs::create_dir_all(&trap).unwrap();
    for program in ["node", "bun", "npx"] {
        let path = trap.join(program);
        std::fs::write(
            &path,
            "#!/bin/sh\nprintf executed >> \"$OC_CFG09_EXEC\"\nexit 97\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let native_alias = global.join("plugin/openproxy-models.js");
    std::fs::create_dir_all(native_alias.parent().unwrap()).unwrap();
    let fifo = std::ffi::CString::new(native_alias.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: isolated owned fixture path. Opening it as plugin code would block
    // the bounded actual child; native alias admission must never open it.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let rejected_file = project.join(format!("{CANARY}.js"));
    std::fs::write(
        &rejected_file,
        format!(
            "require('fs').writeFileSync('{}', 'executed');",
            fixture.root.path().join("file-as-code-executed").display()
        ),
    )
    .unwrap();
    std::fs::write(project.join("AGENTS.md"), "CFG09_NATIVE_RULE_ONCE").unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&source).unwrap()).unwrap();
    let provider = config["provider"]
        .as_object_mut()
        .unwrap()
        .remove("fixture")
        .unwrap();
    let endpoint = provider["options"]["baseURL"].as_str().unwrap().to_string();
    config["provider"]["ludka2"] = provider;
    config["provider"]["ludka2"]["models"] = json!({});
    config["model"] = format!("ludka2/{MODEL}").into();
    config.as_object_mut().unwrap().remove("agent");
    config["plugin"] = json!([
        "@tarquinen/opencode-dcp",
        "@tarquinen/opencode-dcp@3.1.15",
        "@tarquinen/opencode-dcp@latest",
        "@tarquinen/opencode-dcp",
        native_alias,
        native_alias,
        "@tarquinen/opencode-dcp@3.1.16",
        global.join("plugin/openproxy-models.js.bak"),
        rejected_file,
        format!("{endpoint}/plugins/{CANARY}.js?token={CANARY}\x1b[31m"),
        format!("https://user:{CANARY}@example.invalid/{CANARY}.js"),
        "@prevalentware/opencode-goal-plugin@0.1.49"
    ]);
    *fixture.models.lock().unwrap() =
        json!({"object":"list","data":[{"id":MODEL,"opencode":{"name":"CFG09 discovered model"}}]});
    std::fs::write(&source, config.to_string()).unwrap();
    let original = std::fs::read(&source).unwrap();
    let mut pty = spawn(fixture.clone(), &trap);
    pty.wait_visible(READY, DEADLINE);
    assert_eq!(
        fixture.discoveries.load(Ordering::Relaxed),
        1,
        "duplicate aliases did not duplicate discovery setup"
    );
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "inventory before prompt"
    );
    settings(&mut pty, "Unsupported plugin — failed");
    wait_screen_row(&pty, "DCP — active", DEADLINE);
    wait_screen_row(&pty, "OpenProxy models — active", DEADLINE);
    pty.send(b"Unsupported plugin");
    wait_screen_row(&pty, "unsupported_plugin", DEADLINE);
    // Details are derived from the same owner DTO; Enter is read-only.
    pty.send(b"\r");
    pty.wait_visible("current=none", DEADLINE);
    pty.send(b"\x03"); // clear filter before closing Settings
    wait_screen_row(&pty, "Permissions", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Compiled plugins");
    pty.send(b"/model\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "CFG09 discovered model", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    for prompt in ["cfg09 first", "cfg09 second", "cfg09 third"] {
        submit(&mut pty, prompt);
        wait_screen_row(&pty, &format!("echo: {prompt}"), DEADLINE);
        wait_idle(&pty);
    }
    let requests = fixture.wait_requests(3);
    for request in &requests {
        assert_eq!(request["model"], MODEL);
        let fixed = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["role"] == "system" || item["role"] == "developer")
            .map(ToString::to_string)
            .collect::<String>();
        assert_eq!(fixed.matches("CFG09_NATIVE_RULE_ONCE").count(), 1);
        assert_eq!(
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|tool| tool["name"] == "compress")
                .count(),
            1
        );
        assert!(!request.to_string().contains(CANARY));
    }
    pty.send(b"/dcp-compress cfg09 early span\r");
    wait_screen_row(&pty, "compressions 1", DEADLINE);
    pty.send(b"\x1b");
    wait_idle(&pty);
    let requests = fixture.wait_requests(5);
    assert!(
        requests[4]["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "function_call_output")
    );
    submit(&mut pty, "cfg09 after compression");
    assert_eq!(
        last_user_text(&fixture.wait_requests(6)[5]).as_deref(),
        Some("cfg09 after compression")
    );
    wait_idle(&pty);
    assert_eq!(std::fs::read(&source).unwrap(), original);

    config["plugin"] = json!(["@tarquinen/opencode-dcp@3.1.16"]);
    std::fs::write(&source, config.to_string()).unwrap();
    pty.send(b"/reload\r");
    wait_screen_row(&pty, "Configuration reloaded", DEADLINE);
    settings(&mut pty, "Unsupported plugin — failed");
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("DCP — active"))
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Compiled plugins");
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 2);
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert!(!String::from_utf8_lossy(&output).contains(CANARY));
    assert!(!fixture.root.path().join("loader-executed").exists());
    assert!(!fixture.root.path().join("file-as-code-executed").exists());
    {
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        assert_eq!(
            oc_adapters::dcp::load_blocks(&db, "cfg09-native")
                .unwrap()
                .len(),
            1
        );
        let history = db.read_history("cfg09-native").unwrap();
        assert!(!format!("{history:?}").contains(CANARY));
    }

    // Reopen uses current config, not a persisted earlier healthy plugin set.
    let mut reopened = spawn(fixture.clone(), &trap);
    reopened.wait_visible("cfg09 after compression", DEADLINE);
    settings(&mut reopened, "Unsupported plugin — failed");
    assert!(
        !render_screen(&reopened.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("DCP — active"))
    );
    reopened.send(b"\x1b");
    dismissed(&reopened, "Compiled plugins");
    reopened.send(b"/quit\r");
    let (status, output) = reopened.wait_exit(DEADLINE);
    assert!(status.success() && reopened.restored());
    assert!(!String::from_utf8_lossy(&output).contains(CANARY));
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 3);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let env = BTreeMap::from([
            (
                "OPENCODE_CONFIG_DIR".into(),
                global.to_string_lossy().into_owned(),
            ),
            ("OC_FIXTURE_KEY".into(), "fixture-not-a-secret".into()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, diagnostics) =
            oc_adapters::application::spawn_with_env(&project, &fixture.data_dir(), env)
                .await
                .unwrap();
        let inventory = app.catalog().await.unwrap().chrome.plugins;
        assert!(inventory.active_modules.is_empty());
        assert_eq!(inventory.entries[0].status, PluginStatus::Failed);
        assert!(inventory.entries[0].current.is_none());
        assert_eq!(
            inventory.entries[0].diagnostic.as_ref().unwrap().code,
            ServiceCode::UnsupportedPlugin
        );
        assert!(!format!("{inventory:?} {diagnostics:?}").contains(CANARY));
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    });
}
