//! R4A: owned inherited sources and saved real selections through the native binary.
use super::*;
use oc_adapters::storage::Db;
use oc_core::core_app::CoreEvent;
use oc_core::domain::SessionId;
use oc_core::queries::{ServiceCode, SessionSelectionAction as Action};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

const CANARY: &str = "R4A_AUTH_CONFIG_PATH_DO_NOT_RENDER_91f0";
const ACTIVE: &str = "r4a-active";
const RETIRED: &str = "r4a-retired";
const VARIANT: &str = "r4a-variant";

fn config_path(f: &Fixture) -> PathBuf {
    f.root.path().join("home/config/opencode/opencode.json")
}

fn project(f: &Fixture) -> PathBuf {
    f.root.path().join("project")
}

fn owner_env(f: &Fixture) -> BTreeMap<String, String> {
    let home = f.root.path().join("home");
    BTreeMap::from([
        ("HOME".into(), home.to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".into(),
            home.join("config").to_string_lossy().into_owned(),
        ),
        ("OC_FIXTURE_KEY".into(), "fixture-not-a-secret".into()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ])
}

fn prefs(f: &Fixture) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(f.data_dir().join("oc.sqlite")).unwrap();
    let mut stmt = conn
        // Accepted local input now has its own global pref. Assert that value
        // separately; every other saved selection/deck/instruction pref remains
        // byte-identical under restoration and refused generation.
        .prepare("SELECT key,value FROM prefs WHERE key<>'tui.prompt_history.v1' ORDER BY key")
        .unwrap();
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn input_history(f: &Fixture) -> Vec<String> {
    let conn = rusqlite::Connection::open(f.data_dir().join("oc.sqlite")).unwrap();
    let value: String = conn
        .query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB))<=4096 THEN value END FROM prefs WHERE key='tui.prompt_history.v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&value).unwrap()
}

fn effects(f: &Fixture) -> (i64, i64, i64) {
    let conn = rusqlite::Connection::open(f.data_dir().join("oc.sqlite")).unwrap();
    let count = |table| {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    (
        count("turns"),
        count("turn_acceptances"),
        count("tool_operations"),
    )
}

fn main_count(f: &Fixture) -> usize {
    f.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| !title::is_title(r))
        .count()
}

fn assert_private(f: &Fixture, output: &[u8]) {
    let shown = String::from_utf8_lossy(output);
    assert!(!shown.contains(CANARY), "raw source leaked to terminal");
    assert!(!shown.contains("private.invalid"));
    assert!(!shown.contains("fixture-not-a-secret"));
    // The current project Location is intentionally visible in the footer.
    // Check private configuration/instruction/launch sources, not their shared
    // ancestor: otherwise ordinary footer rendering fails under a long TMPDIR.
    for private in [
        f.root.path().join("home/config"),
        f.root.path().join("trap-bin"),
        project(f).join("opencode.jsonc"),
        project(f).join("AGENTS.md"),
        project(f).join(".opencode"),
    ] {
        assert!(
            !shown.contains(private.to_string_lossy().as_ref()),
            "private source path leaked to terminal"
        );
    }
}

fn home_pty(f: Arc<Fixture>) -> PtySession {
    let (master, slave) = openpty_pair(120, 40);
    let mut cmd = f.command();
    cmd.arg("tui")
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(dup_fd(&slave)))
        .stdout(Stdio::from(dup_fd(&slave)))
        .stderr(Stdio::from(dup_fd(&slave)));
    terminal::controlling_terminal(&mut cmd);
    let child = cmd.spawn().unwrap();
    drop(slave);
    PtySession::finish_spawn(master, child, f)
}

fn quit(pty: &mut PtySession) -> Vec<u8> {
    pty.send(b"/exit\r");
    let (status, bytes) = pty.wait_exit(DEADLINE);
    assert_eq!(status.code(), Some(0));
    assert!(pty.restored());
    assert!(
        bytes
            .windows(ALT_LEAVE.len())
            .any(|bytes| bytes == ALT_LEAVE)
    );
    bytes
}

fn setup(f: &Fixture) -> PathBuf {
    let path = config_path(f);
    let mut cfg: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    cfg["default_agent"] = "build".into();
    cfg["agent"] = json!({
        "build":{"mode":"primary", "prompt":"R4A_ADMITTED_BUILD"},
        "lost":{"mode":"primary", "model":format!("fixture/{MODEL}"), "prompt":"R4A_LOST_BODY"},
        "retired":{"mode":"primary", "model":format!("fixture/{ALT_MODEL}#fast"), "prompt":"R4A_RETIRED_BODY"},
        "variant":{"mode":"primary", "model":format!("fixture/{MODEL}#fast"), "prompt":"R4A_VARIANT_BODY"}
    });
    cfg["provider"]["fixture"]["models"][MODEL]["variants"]["fast"] =
        json!({"reasoningEffort":"high"});
    cfg["plugin"] = json!([
        "@tarquinen/opencode-dcp",
        format!("https://user:{CANARY}@private.invalid/{CANARY}\u{1b}[31m")
    ]);
    let bin = f.root.path().join("trap-bin");
    std::fs::create_dir(&bin).unwrap();
    let marker = bin.join("spawned");
    let trap = bin.join("npx");
    std::fs::write(
        &trap,
        format!("#!/bin/sh\nprintf 'unexpected' > {:?}\nexit 99\n", marker),
    )
    .unwrap();
    std::fs::set_permissions(&trap, std::fs::Permissions::from_mode(0o700)).unwrap();
    cfg["mcp"] = json!({"chrome-devtools":{"type":"local", "command":[trap,"-y","chrome-devtools-mcp@latest"],
        "enabled":false,"environment":{"R4A_AUTH": CANARY},"cwd":"missing-disabled-directory","timeout":5000}});
    std::fs::write(&path, cfg.to_string()).unwrap();
    std::fs::write(
        project(f).join("opencode.jsonc"),
        json!({"permissions":{"read":"deny"}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        project(f).join("AGENTS.md"),
        "R4A_PROJECT_SOURCE_INSTRUCTIONS",
    )
    .unwrap();
    let commands = project(f).join(".opencode/command");
    std::fs::create_dir_all(&commands).unwrap();
    std::fs::write(
        commands.join("audit.md"),
        "---\ndescription: R4A local command\n---\nR4A local command $1",
    )
    .unwrap();
    std::fs::write(
        path.parent().unwrap().join("dcp.jsonc"),
        format!("{{\"{CANARY}\":true}}"),
    )
    .unwrap();
    marker
}

fn seed(f: &Fixture) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (app, guard, _) =
                oc_adapters::application::spawn_with_env(&project(f), &f.data_dir(), owner_env(f))
                    .await
                    .unwrap();
            let active = SessionId::new(ACTIVE).unwrap();
            let retired = SessionId::new(RETIRED).unwrap();
            let variant = SessionId::new(VARIANT).unwrap();
            for session in [&active, &retired, &variant] {
                app.create_session(session.clone()).await.unwrap();
            }
            app.select_agent("lost".into()).await.unwrap();
            for (id, agent) in [
                (&active, "lost"),
                (&retired, "retired"),
                (&variant, "variant"),
            ] {
                app.session_selection(id.clone(), false, Action::Agent(agent.into()))
                    .await
                    .unwrap();
            }
            app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
                sessions: vec![active.clone(), retired.clone(), variant.clone()],
                active: Some(active.clone()),
                ..app.tab_deck().await.unwrap()
            })
            .await
            .unwrap();
            let mut events = app.subscribe();
            app.submit(active.clone(), "r4a seeded history".into())
                .await
                .unwrap();
            loop {
                match tokio::time::timeout(DEADLINE, events.recv())
                    .await
                    .unwrap()
                    .unwrap()
                {
                    CoreEvent::TurnFinished { session, .. } if session == active => break,
                    CoreEvent::TurnFailed { error, .. } => panic!("owned seed failed: {error}"),
                    _ => {}
                }
            }
            app.shutdown().await.unwrap();
            guard.join_diagnostic().await.unwrap();
        });
    assert_eq!(main_count(f), 1);
}

fn retire(f: &Fixture) {
    let path = config_path(f);
    let mut cfg: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    cfg["agent"].as_object_mut().unwrap().remove("lost");
    cfg["provider"]["fixture"]["models"]
        .as_object_mut()
        .unwrap()
        .remove(ALT_MODEL);
    cfg["provider"]["fixture"]["models"][MODEL]["variants"]["fast"]["disabled"] = true.into();
    std::fs::write(path, cfg.to_string()).unwrap();
}

#[test]
fn r4a_native_inherited_active_parked_home_refusal_repair_and_restart() {
    let fixture = Fixture::new();
    let trap = setup(&fixture);
    seed(&fixture);
    retire(&fixture);
    let saved = prefs(&fixture);
    let old_effects = effects(&fixture);
    let old_main = main_count(&fixture);
    let old_requests = fixture.requests.lock().unwrap().len();
    assert_eq!(old_main, 1);
    assert_eq!(old_effects.0, 1);
    let mut home = home_pty(fixture.clone());
    wait_screen_row(&home, "Native runtime", DEADLINE);
    inspect_settings_code(&mut home, "agent_unavailable");
    wait_idle(&home);
    home.send(b"/agents\r");
    wait_screen_row(&home, "Saved agent agent-", DEADLINE);
    wait_screen_row(&home, "Select agent", DEADLINE);
    home.send(b"\x1b");
    dismissed(&home, "Select agent");
    home.send(b"/sessions\r");
    wait_screen_row(&home, "Fixture session title", DEADLINE);
    home.send(b"\x1b");
    dismissed(&home, "Sessions for project");
    wait_idle(&home);
    let rendered = quit(&mut home);
    assert_private(&fixture, &rendered);
    assert_eq!(
        prefs(&fixture),
        saved,
        "Home restoration rewrote an inherited selection or deck"
    );
    let accepted_local_input = input_history(&fixture);
    assert_eq!(
        accepted_local_input,
        [
            "r4a seeded history",
            "/settings",
            "/agents",
            "/sessions",
            "/exit"
        ]
    );
    assert_eq!(effects(&fixture), old_effects);
    assert_eq!(main_count(&fixture), old_main);
    assert_eq!(
        fixture.requests.lock().unwrap().len(),
        old_requests,
        "restoration sent a main or title request"
    );
    assert!(!trap.exists(), "disabled MCP spawned an executable");
    let refused_home = fixture
        .command()
        .args(["run", "--json", "r4a Home refused before creation"])
        .output()
        .unwrap();
    assert_eq!(refused_home.status.code(), Some(1));
    assert!(refused_home.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&refused_home.stderr);
    assert!(stderr.contains("agent_unavailable") && stderr.contains("selection.agent"));
    assert!(stderr.contains("source-") && stderr.contains("select an admitted primary agent"));
    assert_private(&fixture, &refused_home.stderr);
    assert_eq!(effects(&fixture), old_effects);
    assert_eq!(main_count(&fixture), old_main);
    assert_eq!(
        fixture.requests.lock().unwrap().len(),
        old_requests,
        "unavailable Home started a main or title request"
    );
    for (id, code, field) in [
        (ACTIVE, "agent_unavailable", "selection.agent"),
        (RETIRED, "model_unavailable", "selection.model"),
        (VARIANT, "variant_unavailable", "selection.variant"),
    ] {
        let result = fixture
            .command()
            .args(["run", "--json", "--session", id, "r4a refused"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(code) && stderr.contains(field), "{stderr}");
        assert!(stderr.contains("source-") && stderr.contains("retryable=false"));
        assert_private(&fixture, &result.stderr);
        assert_eq!(effects(&fixture), old_effects);
        assert_eq!(main_count(&fixture), old_main);
        assert_eq!(
            fixture.requests.lock().unwrap().len(),
            old_requests,
            "unavailable tab started a main or title request"
        );
    }
    assert_eq!(
        prefs(&fixture),
        saved,
        "headless refusal rewrote saved selections"
    );
    assert_eq!(input_history(&fixture), accepted_local_input);
    // The Home choice is explicit and Location-scoped. Repairing it does not
    // silently repair the three independently saved session selections.
    let mut home = home_pty(fixture.clone());
    wait_screen_row(&home, "Native runtime", DEADLINE);
    home.send(b"/agents\r");
    wait_screen_row(&home, "Saved agent agent-", DEADLINE);
    home.send(b"\x1b[B\r");
    dismissed(&home, "Select agent");
    wait_idle(&home);
    submit(&mut home, "r4a home explicitly repaired");
    wait_screen_row(&home, "echo: r4a home explicitly repaired", DEADLINE);
    let home_request = fixture.wait_requests(old_main + 1);
    assert_eq!(home_request.last().unwrap()["model"], MODEL);
    assert!(
        home_request
            .last()
            .unwrap()
            .to_string()
            .contains("R4A_ADMITTED_BUILD")
    );
    let output = quit(&mut home);
    assert_private(&fixture, &output);
    assert_eq!(main_count(&fixture), old_main + 1);
    assert!(
        prefs(&fixture)
            .iter()
            .any(|(key, value)| key.contains("home_agent") && value == "\"build\"")
    );

    let mut retired = PtySession::spawn_sized(fixture.clone(), RETIRED, None, 120, 40);
    inspect_settings_code(&mut retired, "model_unavailable");
    retired.send(b"/model\r");
    wait_screen_row(&retired, "Select model", DEADLINE);
    wait_screen_row(&retired, "T39 model", DEADLINE);
    retired.send(b"T39 model\r");
    dismissed(&retired, "Select model");
    wait_idle(&retired);
    submit(&mut retired, "r4a model explicitly repaired");
    wait_screen_row(&retired, "echo: r4a model explicitly repaired", DEADLINE);
    assert_eq!(
        fixture.wait_requests(old_main + 2).last().unwrap()["model"],
        MODEL
    );
    let output = quit(&mut retired);
    assert_private(&fixture, &output);

    let mut variant = PtySession::spawn_sized(fixture.clone(), VARIANT, None, 120, 40);
    inspect_settings_code(&mut variant, "variant_unavailable");
    variant.send(b"/variants\r");
    wait_screen_row(&variant, "Select variant", DEADLINE);
    wait_screen_row(&variant, "Default", DEADLINE);
    variant.send(b"\r");
    dismissed(&variant, "Select variant");
    wait_idle(&variant);
    submit(&mut variant, "r4a variant explicitly repaired");
    wait_screen_row(&variant, "echo: r4a variant explicitly repaired", DEADLINE);
    assert_eq!(
        fixture.wait_requests(old_main + 3).last().unwrap()["model"],
        MODEL
    );
    let output = quit(&mut variant);
    assert_private(&fixture, &output);

    let mut active = PtySession::spawn_sized(fixture.clone(), ACTIVE, None, 120, 40);
    wait_screen_row(&active, "echo: r4a seeded history", DEADLINE);
    inspect_settings_code(&mut active, "agent_unavailable");
    submit(&mut active, "r4a retained refused draft");
    wait_screen_row(&active, "Request unavailable: agent_unavailable", DEADLINE);
    wait_screen_row(&active, "agent_unavailable", DEADLINE);
    wait_screen_row(&active, "r4a retained refused draft", DEADLINE);
    // Inspect the captured refusal while the ordinary draft is still unsent.
    // Palette/modal filtering is not a request, selection repair or grant.
    active.send(b"\x10");
    wait_screen_row(&active, "Commands", DEADLINE);
    active.send(b"Settings\r");
    wait_screen_row(&active, "Settings", DEADLINE);
    wait_screen_row(&active, "Last request", DEADLINE);
    active.send(b"Last request\r");
    // The code already appeared in the brief refusal/list. This field proves
    // the explicit captured detail completed before dismissing its surface.
    wait_screen_row(&active, "selection.agent", DEADLINE);
    active.send(b"\x1b");
    // Settings' first Escape clears its nonempty modal filter; the second
    // dismisses it. The real empty-search placeholder is the input barrier.
    wait_screen_row(&active, "Search", DEADLINE);
    active.send(b"\x1b");
    dismissed(&active, "Settings");
    wait_screen_row(&active, "r4a retained refused draft", DEADLINE);
    let before_active = effects(&fixture);
    assert_eq!(before_active.0, old_effects.0 + 3);
    assert_eq!(main_count(&fixture), old_main + 3);
    active.send(b"\x03");
    wait_idle(&active);
    active.send(b"/agents\r");
    wait_screen_row(&active, "Saved agent agent-", DEADLINE);
    active.send(b"\x1b[B\r");
    dismissed(&active, "Select agent");
    wait_idle(&active);
    submit(&mut active, "r4a repaired exact wire");
    wait_screen_row(&active, "echo: r4a repaired exact wire", DEADLINE);
    let requests = fixture.wait_requests(old_main + 4);
    let repaired = requests.last().unwrap();
    assert_eq!(repaired["model"], MODEL);
    assert!(repaired.to_string().contains("R4A_ADMITTED_BUILD"));
    assert!(
        repaired
            .to_string()
            .contains("R4A_PROJECT_SOURCE_INSTRUCTIONS")
    );
    assert!(!repaired.to_string().contains("R4A_LOST_BODY"));
    assert!(!repaired.to_string().contains(CANARY));
    assert_eq!(
        repaired["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["name"] == "compress")
            .count(),
        1
    );
    std::fs::write(project(&fixture).join("cfg10-denied-note.txt"), CANARY).unwrap();
    wait_idle(&active);
    submit(&mut active, fatal_diagnostics::POLICY_PROMPT);
    wait_screen_row(&active, "answer:compressed", DEADLINE);
    let continuation = fixture.wait_requests(old_main + 6);
    assert!(
        continuation.last().unwrap()["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|part| part["type"] == "function_call_output"
                && part["output"]
                    .as_str()
                    .is_some_and(|text| text.contains("denied read")))
    );
    assert!(!continuation.last().unwrap().to_string().contains(CANARY));
    let output = quit(&mut active);
    assert_private(&fixture, &output);
    assert!(!trap.exists());
    let repair_effects = effects(&fixture);
    assert_eq!(repair_effects.0, old_effects.0 + 5);
    assert_eq!(repair_effects.2, old_effects.2 + 1);
    let result = fixture
        .command()
        .args([
            "run",
            "--json",
            "--session",
            ACTIVE,
            "r4a scoped headless after restart",
        ])
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("\"type\":\"done\""));
    assert_private(&fixture, &result.stderr);
    let requests = fixture.wait_requests(old_main + 7);
    assert_eq!(requests.last().unwrap()["model"], MODEL);
    let mut reopened = PtySession::spawn_sized(fixture.clone(), ACTIVE, None, 120, 40);
    wait_screen_row(&reopened, "echo: r4a repaired exact wire", DEADLINE);
    wait_screen_row(&reopened, "r4a scoped headless after restart", DEADLINE);
    // The fixture replies to a history with a completed tool result with
    // answer:compressed; the owner must still route the exact next user/model.
    wait_screen_row(&reopened, "answer:compressed", DEADLINE);
    let output = quit(&mut reopened);
    assert_private(&fixture, &output);
    assert_eq!(main_count(&fixture), old_main + 7);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
    assert!(!trap.exists());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (app, guard, _) = oc_adapters::application::spawn_with_env(
                &project(&fixture),
                &fixture.data_dir(),
                owner_env(&fixture),
            )
            .await
            .unwrap();
            let active = SessionId::new(ACTIVE).unwrap();
            let history = app
                .history_page(active.clone(), None, None, 30)
                .await
                .unwrap();
            let last_user = history
                .rows
                .iter()
                .rfind(|row| row.role == oc_core::session::Role::User)
                .unwrap();
            let fork = app
                .fork_session(active.clone(), last_user.id.clone())
                .await
                .unwrap();
            assert_eq!(fork.prompt, "r4a scoped headless after restart");
            let choice = app
                .session_selection(fork.session.clone(), false, Action::Current)
                .await
                .unwrap();
            assert_eq!(choice.agent_id.as_deref(), Some("build"));
            assert_eq!(choice.model_id, MODEL);
            assert!(choice.chrome.selection.is_none());
            let other = fixture.root.path().join("other-project");
            std::fs::create_dir(&other).unwrap();
            std::fs::write(
                other.join("opencode.jsonc"),
                json!({"permissions":{"read":"deny"}}).to_string(),
            )
            .unwrap();
            app.switch_location_home(other.to_string_lossy().into_owned())
                .await
                .unwrap();
            let home = app.home_selection(Action::Current).await.unwrap();
            assert_eq!(
                home.chrome.selection.unwrap().diagnostic.code,
                ServiceCode::AgentUnavailable
            );
            let valid = app
                .home_selection(Action::Agent("build".into()))
                .await
                .unwrap();
            assert!(valid.chrome.selection.is_none());
            app.switch_location_home(project(&fixture).to_string_lossy().into_owned())
                .await
                .unwrap();
            let old = app.home_selection(Action::Current).await.unwrap();
            assert_eq!(old.agent_id.as_deref(), Some("build"));
            assert!(old.chrome.selection.is_none());
            assert!(
                app.session_selection(active, false, Action::Current)
                    .await
                    .unwrap()
                    .chrome
                    .selection
                    .is_none()
            );
            assert!(
                app.session_selection(fork.session, false, Action::Current)
                    .await
                    .unwrap()
                    .chrome
                    .selection
                    .is_none()
            );
            app.shutdown().await.unwrap();
            guard.join_diagnostic().await.unwrap();
        });
    let db = Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        db.list_sessions().unwrap().len(),
        5,
        "three source tabs, explicit Home root and fork"
    );
}

#[test]
fn r4a_native_existing_primary_blocked_pin_is_model_cause_and_repairs_without_agent_fallback() {
    let fixture = Fixture::new();
    let trap = setup(&fixture);
    let path = config_path(&fixture);
    let mut cfg: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    cfg["agent"]["lost"]["model"] = format!("fixture/{ALT_MODEL}").into();
    std::fs::write(&path, cfg.to_string()).unwrap();
    seed(&fixture);
    cfg["agent"]["lost"]["model"] = format!("foreign/{CANARY}\u{1b}[31m").into();
    cfg["provider"]["fixture"]["models"][MODEL]["variants"]["fast"]["disabled"] = true.into();
    cfg["provider"]["fixture"]["models"]
        .as_object_mut()
        .unwrap()
        .remove(ALT_MODEL);
    std::fs::write(&path, cfg.to_string()).unwrap();
    let saved = prefs(&fixture);
    let old_effects = effects(&fixture);
    let old_requests = fixture.requests.lock().unwrap().len();

    let mut home = home_pty(fixture.clone());
    wait_screen_row(&home, "Native runtime", DEADLINE);
    inspect_settings_code(&mut home, "model_unavailable");
    wait_idle(&home);
    home.send(b"/agents\r");
    wait_screen_row(&home, "Select agent", DEADLINE);
    wait_screen_row(&home, "lost", DEADLINE);
    assert!(!String::from_utf8_lossy(&home.snapshot()).contains("Saved agent agent-"));
    home.send(b"\x1b");
    dismissed(&home, "Select agent");
    let output = quit(&mut home);
    assert_private(&fixture, &output);
    let mut active = PtySession::spawn_sized(fixture.clone(), ACTIVE, None, 120, 40);
    wait_screen_row(&active, "echo: r4a seeded history", DEADLINE);
    inspect_settings_code(&mut active, "model_unavailable");
    submit(&mut active, "r4a blocked pin retained draft");
    wait_screen_row(&active, "Request unavailable: model_unavailable", DEADLINE);
    wait_screen_row(&active, "r4a blocked pin retained draft", DEADLINE);
    active.send(b"\x03");
    wait_idle(&active);
    let output = quit(&mut active);
    assert_private(&fixture, &output);
    for scope in [None, Some(ACTIVE), Some(RETIRED)] {
        let mut command = fixture.command();
        command.args(["run", "--json"]);
        if let Some(id) = scope {
            command.args(["--session", id]);
        }
        let result = command
            .arg("blocked pin refused before effects")
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("model_unavailable") && stderr.contains("selection.model"));
        assert!(!stderr.contains("agent_unavailable"));
        assert!(stderr.contains("source-") && stderr.contains("select an admitted model"));
        assert_private(&fixture, &result.stderr);
    }
    assert_eq!(prefs(&fixture), saved);
    assert_eq!(
        input_history(&fixture),
        [
            "r4a seeded history",
            "/settings",
            "/agents",
            "/exit",
            "/settings",
            "r4a blocked pin retained draft",
            "/exit",
        ],
        "only accepted local commands and explicitly retained clear enter input history"
    );
    assert_eq!(effects(&fixture), old_effects);
    assert_eq!(fixture.requests.lock().unwrap().len(), old_requests);
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
    assert!(!trap.exists());

    // Repair the model in Home and the active tab separately, retaining the
    // existing primary's body and constraints. The parked tab remains blocked.
    let mut home = home_pty(fixture.clone());
    inspect_settings_code(&mut home, "model_unavailable");
    wait_idle(&home);
    home.send(b"/model\r");
    wait_screen_row(&home, "Select model", DEADLINE);
    home.send(b"T39 model\r");
    dismissed(&home, "Select model");
    wait_idle(&home);
    let output = quit(&mut home);
    assert_private(&fixture, &output);
    let mut active = PtySession::spawn_sized(fixture.clone(), ACTIVE, None, 120, 40);
    inspect_settings_code(&mut active, "model_unavailable");
    wait_idle(&active);
    active.send(b"/model\r");
    wait_screen_row(&active, "Select model", DEADLINE);
    active.send(b"T39 model\r");
    dismissed(&active, "Select model");
    wait_idle(&active);
    submit(&mut active, "r4a blocked pin explicitly repaired");
    wait_screen_row(
        &active,
        "echo: r4a blocked pin explicitly repaired",
        DEADLINE,
    );
    let requests = fixture.wait_requests(2);
    let wire = requests.last().unwrap();
    assert_eq!(wire["model"], MODEL);
    assert!(wire.to_string().contains("R4A_LOST_BODY"));
    assert!(!wire.to_string().contains("R4A_ADMITTED_BUILD"));
    assert!(!wire.to_string().contains(CANARY));
    std::fs::write(project(&fixture).join("cfg10-denied-note.txt"), CANARY).unwrap();
    wait_idle(&active);
    submit(&mut active, fatal_diagnostics::POLICY_PROMPT);
    wait_screen_row(&active, "answer:compressed", DEADLINE);
    let requests = fixture.wait_requests(4);
    assert!(
        requests.last().unwrap()["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|part| part["type"] == "function_call_output"
                && part["output"]
                    .as_str()
                    .is_some_and(|s| s.contains("denied read")))
    );
    assert!(!requests.last().unwrap().to_string().contains(CANARY));
    let output = quit(&mut active);
    assert_private(&fixture, &output);
    let result = fixture
        .command()
        .args([
            "run",
            "--json",
            "--session",
            ACTIVE,
            "r4a blocked pin reopened headless",
        ])
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("\"type\":\"done\""));
    assert_private(&fixture, &result.stderr);
    assert_eq!(fixture.wait_requests(5).last().unwrap()["model"], MODEL);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (app, guard, _) = oc_adapters::application::spawn_with_env(
                &project(&fixture),
                &fixture.data_dir(),
                owner_env(&fixture),
            )
            .await
            .unwrap();
            let home = app.home_selection(Action::Current).await.unwrap();
            assert_eq!(home.agent_id.as_deref(), Some("lost"));
            assert_eq!(home.chrome.selection.unwrap().diagnostic.code, ServiceCode::ModelUnavailable,
                "committing a captured fresh session does not rewrite independent Home/config preferences");
            let active = app.session_selection(SessionId::new(ACTIVE).unwrap(), false, Action::Current).await.unwrap();
            assert_eq!(active.agent_id.as_deref(), Some("lost"));
            assert_eq!(active.model_id, MODEL);
            assert!(active.chrome.selection.is_none());
            let parked = app
                .session_selection(SessionId::new(RETIRED).unwrap(), false, Action::Current)
                .await
                .unwrap();
            assert_eq!(
                parked.chrome.selection.unwrap().diagnostic.code,
                ServiceCode::ModelUnavailable
            );
            app.shutdown().await.unwrap();
            guard.join_diagnostic().await.unwrap();
        });
    assert_eq!(main_count(&fixture), 5);
    assert_eq!(
        fixture.requests.lock().unwrap().len(),
        6,
        "five main plus the seed title"
    );
    assert_eq!(fixture.discoveries.load(Ordering::Relaxed), 0);
    assert_eq!(
        effects(&fixture),
        (old_effects.0 + 3, old_effects.1 + 3, old_effects.2 + 1)
    );
    assert!(!trap.exists());
}
