//! Catalog-only consumer against a real native store owned by another handle.
use super::*;
use oc_adapters::storage::Db;

#[test]
fn go05_configless_headless_refuses_before_session_turn_or_tools() {
    let fixture = Fixture::new();
    let data = fixture.home.join("unchosen-data");
    let db = Db::open(&data).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    db.set_pref(
        "public-catalog:https://models.dev/api.json:opencode-go:v1",
        &json!({
            "source":oc_adapters::models_dev::SOURCE,"fetched_at_ms":now,
            "record":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{}}
        })
        .to_string(),
    )
    .unwrap();
    drop(db);
    let stdout = fixture.home.join("unchosen.stdout");
    let stderr = fixture.home.join("unchosen.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_oc"))
        .env_clear()
        .env("HOME", &fixture.home)
        .env("OPENCODE_CONFIG_DIR", &fixture.global)
        .env("PATH", &fixture.trap_bin)
        .current_dir(&fixture.project_a)
        .arg("--data-dir")
        .arg(&data)
        .arg("run")
        .arg("must not execute")
        .arg("--session")
        .arg("unchosen-root")
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut process = Process {
        child,
        stdout,
        stderr,
        titles: 0,
    };
    assert!(!process.wait().success());
    assert!(process.output().is_empty());
    assert!(process.diagnostics().contains("model_unavailable"));
    let db = Db::open(&data).unwrap();
    assert!(db.list_sessions().unwrap().is_empty());
    drop(db);
    let conn = rusqlite::Connection::open_with_flags(
        data.join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM turns", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    fixture.assert_no_request();
    assert!(!fixture.exec_log.exists());
}

#[test]
fn tool18_models_does_not_open_locked_native_history_prefs_or_recover_pending_turn() {
    let fixture = Fixture::new();
    let data = fixture.home.join("native-data");
    let db = Db::open(&data).unwrap();
    db.create_session("catalog-existing").unwrap();
    db.append_message("catalog-existing", "user", "immutable history")
        .unwrap();
    db.set_prefs(&[("tui.model".into(), "unavailable/retired".into())])
        .unwrap();
    db.begin_turn("catalog-pending", "catalog-existing", "must not recover")
        .unwrap();
    // A live owner's committed WAL cache is readable without touching any store
    // file. Public Go is independent of unavailable selection/credentials.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    db.set_pref(
        "public-catalog:https://models.dev/api.json:opencode-go:v1",
        &json!({
            "source":oc_adapters::models_dev::SOURCE,"fetched_at_ms":now,
            "record":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{
                "cached/public":{"id":"cached/public","name":"Cached Public",
                    "limit":{"context":10000,"output":1024},"tool_call":true}
            }}
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        fixture.project_a.join("opencode.json"),
        json!({
            "model":"unavailable/retired", "default_agent":"unavailable",
            "provider":{"fixture":{"models":{"route/model":{}}}}
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        fixture.global.join("cli.json"),
        "not valid preferences; catalog must not read",
    )
    .unwrap();
    let snapshot = || {
        let mut files: Vec<_> = fs::read_dir(&data)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .map(|path| (path.clone(), fs::read(&path).unwrap()))
            .collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    };
    let before = snapshot();
    let stdout = fixture.home.join("catalog.stdout");
    let stderr = fixture.home.join("catalog.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_oc"))
        .env_clear()
        .env("HOME", &fixture.home)
        .env("OPENCODE_CONFIG_DIR", &fixture.global)
        .env("PATH", &fixture.trap_bin)
        .current_dir(&fixture.project_a)
        .arg("--data-dir")
        .arg(&data)
        .arg("models")
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut process = Process {
        child,
        stdout,
        stderr,
        titles: 0,
    };
    assert!(process.wait().success(), "{}", process.diagnostics());
    assert_eq!(
        process.output(),
        "fixture/route/model\nopencode-go/cached/public\n"
    );
    assert!(process.diagnostics().is_empty());
    assert_eq!(snapshot(), before, "native store/recovery mutated");
    assert_eq!(
        db.get_pref("tui.model").unwrap().as_deref(),
        Some("unavailable/retired")
    );
    assert_eq!(db.history_len("catalog-existing").unwrap(), 1);
    fixture.assert_no_request();
    assert!(!fixture.exec_log.exists());
    assert!(
        !fixture.home.join(".local").exists(),
        "catalog wrote startup trace"
    );
}
