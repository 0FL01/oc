//! CFG10: optional definition authority, fatal admission and complete rollback.
use super::*;
use oc_core::queries::{ServiceAction, ServiceCode, ServiceKind, ServiceStage};
use std::path::PathBuf;

const CANARY: &str = "CFG10_CONFIG_ENV_AUTH_PATH_SECRET_4219";

struct Fixture {
    _root: tempfile::TempDir,
    project: PathBuf,
    global: PathBuf,
    data: PathBuf,
    env: BTreeMap<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let global = root.path().join(format!("{CANARY}\u{1b}[31m/opencode"));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&global).unwrap();
        let data = root.path().join(format!("{CANARY}-data"));
        let config = serde_json::json!({
            "model": "fixture/main", "permissions": {"read": "deny"},
            "provider": {"fixture": {"options": {"baseURL": "https://example.invalid/v1", "apiKey": CANARY},
                "models": {"main": {}}}},
        });
        std::fs::write(global.join("opencode.json"), config.to_string()).unwrap();
        std::fs::write(
            project.join("AGENTS.md"),
            "CFG10 healthy instruction generation",
        )
        .unwrap();
        let env = BTreeMap::from([(
            "XDG_CONFIG_HOME".into(),
            global.parent().unwrap().to_string_lossy().into_owned(),
        )]);
        Self {
            _root: root,
            project,
            global,
            data,
            env,
        }
    }

    fn safe(diagnostic: &oc_core::queries::ServiceDiagnostic) {
        let shown = format!(
            "{diagnostic}\n{diagnostic:?}\n{}\n{}",
            serde_json::to_string(diagnostic).unwrap(),
            diagnostic.investigation_draft()
        );
        assert!(!shown.contains(CANARY), "{shown}");
        assert!(!shown.contains("\u{1b}"));
        assert!(!shown.contains("https://"));
        assert!(diagnostic.source.starts_with("source-"));
        assert!(!diagnostic.field.is_empty());
    }
}

#[tokio::test]
async fn cfg10_optional_definition_failure_is_safe_but_policy_document_is_fatal_and_atomic() {
    let fixture = Fixture::new();
    let commands = fixture.global.join("commands");
    std::fs::create_dir_all(&commands).unwrap();
    std::fs::write(commands.join(format!("{CANARY}.md")),
        format!("---\n{CANARY}: [unterminated\n---\nhttps://user:{CANARY}@private.invalid/{CANARY}\u{1b}[31m")).unwrap();
    std::fs::write(
        commands.join("healthy.md"),
        "---\ndescription: Healthy command\n---\nhello $1",
    )
    .unwrap();
    std::fs::write(
        fixture.global.join("dcp.jsonc"),
        format!("{{\"{CANARY}\":true}}"),
    )
    .unwrap();
    let path = fixture.global.join("opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["command"] = serde_json::Value::Object(
        (0..130)
            .map(|index| (format!("{CANARY}-{index}"), serde_json::Value::Null))
            .collect(),
    );
    config["provider"]["fixture"]["options"][CANARY] = serde_json::json!(CANARY);
    config["mcp"][CANARY] = serde_json::Value::Null;
    std::fs::write(path, config.to_string()).unwrap();
    let composed = composition::load_with_env(&fixture.project, fixture.env.clone())
        .await
        .unwrap();
    assert_eq!(
        composed.generation.permissions["read"],
        crate::config::Permission::Deny
    );
    assert!(
        composed
            .instructions
            .contains("CFG10 healthy instruction generation")
    );
    assert!(composed.commands.contains_key("healthy"));
    assert!(!composed.commands.contains_key(CANARY));
    assert_eq!(
        composed
            .tui_chrome
            .service_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind == ServiceKind::Definition)
            .count(),
        64
    );
    assert_eq!(composed.tui_chrome.service_diagnostics_omitted, 67);
    assert!(
        composed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("67 additional"))
    );
    let diagnostic = composed
        .tui_chrome
        .service_diagnostics
        .iter()
        .find(|d| d.kind == ServiceKind::Definition)
        .unwrap();
    assert_eq!(diagnostic.code, ServiceCode::InvalidDefinition);
    assert_eq!(diagnostic.stage, ServiceStage::Config);
    assert_eq!(diagnostic.field, ["command", "entry"]);
    assert_eq!(diagnostic.action, ServiceAction::ReviewConfiguration);
    Fixture::safe(diagnostic);
    assert!(!format!("{:?}", composed.diagnostics).contains(CANARY));
    assert!(!format!("{:?}", composed.generation.warnings).contains(CANARY));
    let (app, guard, _) = spawn_with_env(&fixture.project, &fixture.data, fixture.env.clone())
        .await
        .unwrap();
    let complete = app.catalog().await.unwrap();
    assert!(!format!("{:?}", app.mcp_status().await.unwrap()).contains(CANARY));
    let suggestions = app.file_suggestions("".into(), 1).await.unwrap();
    let policy = fixture.project.join("opencode.jsonc");
    std::fs::write(
        &policy,
        format!("{{\"apiKey\":\"{CANARY}\",\"permissions\":{{\"read\":"),
    )
    .unwrap();
    let error = app.reload_location().await.unwrap_err();
    let CoreError::LocationSwitch {
        diagnostic: Some(diagnostic),
        category,
        ..
    } = error
    else {
        panic!("structured refusal required")
    };
    assert_eq!(category, LocationSwitchFailure::Configuration);
    assert_eq!(diagnostic.code, ServiceCode::InvalidDocument);
    assert_eq!(diagnostic.field, ["document"]);
    assert_eq!(
        diagnostic.source,
        crate::config::mcp::safe_source_id(&policy.to_string_lossy())
    );
    Fixture::safe(&diagnostic);
    assert_eq!(app.catalog().await.unwrap(), complete);
    assert_eq!(
        app.file_suggestions("".into(), 1).await.unwrap().generation,
        suggestions.generation
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    // Complete global policy cannot authorize skipping a broken lower-trust source.
    let error = spawn_with_env(&fixture.project, &fixture.data, fixture.env.clone())
        .await
        .err()
        .expect("fatal document");
    assert!(error.contains("invalid_document"));
    assert!(!error.contains(CANARY));
}

#[tokio::test]
async fn cfg10_corrupt_saved_selection_is_non_success_not_default_fallback() {
    let fixture = Fixture::new();
    let db = Db::open(&fixture.data).unwrap();
    db.set_pref(
        oc_core::queries::PREF_MODEL_SELECTION,
        &format!("{{\"provider\":\"fixture\",\"id\":\"{CANARY}\""),
    )
    .unwrap();
    drop(db);
    let result = spawn_stages(&fixture.project, &fixture.data, fixture.env, false).await;
    match result {
        Err(issue) => {
            assert_eq!(issue.category, SpawnFailure::Storage);
            assert_eq!(issue.diagnostic.stage, ServiceStage::Query);
            assert_eq!(issue.diagnostic.field, ["selection", "model"]);
            Fixture::safe(&issue.diagnostic);
        }
        Ok((app, guard, _, _)) => {
            app.shutdown().await.unwrap();
            guard.join().await.unwrap();
            panic!("malformed persisted choice must not authorize the default model");
        }
    }
}

#[tokio::test]
async fn cfg10_retained_root_query_storage_faults_keep_safe_typed_causes() {
    let fixture = Fixture::new();
    let (app, guard, _) = spawn_with_env(&fixture.project, &fixture.data, fixture.env)
        .await
        .unwrap();
    let session = SessionId::new("cfg10-query").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let conn = rusqlite::Connection::open(fixture.data.join("oc.sqlite")).unwrap();
    conn.execute_batch("DROP TABLE dcp_accounting; DROP TABLE tool_operations;")
        .unwrap();
    let errors = [
        app.dcp_snapshot(session.clone()).await.unwrap_err(),
        app.tool_ops_page(session, None, 8).await.unwrap_err(),
    ];
    for error in errors {
        let CoreError::Diagnostic(diagnostic) = error else {
            panic!("retained root storage fault lost the typed diagnostic");
        };
        assert_eq!(diagnostic.kind, ServiceKind::Storage);
        assert_eq!(diagnostic.stage, ServiceStage::Query);
        assert_eq!(diagnostic.code, ServiceCode::StorageUnavailable);
        assert_eq!(diagnostic.action, ServiceAction::ReviewStorage);
        assert_eq!(diagnostic.field, ["database"]);
        assert_eq!(
            diagnostic.source,
            crate::config::mcp::safe_source_id(&fixture.data.to_string_lossy())
        );
        Fixture::safe(&diagnostic);
    }
    let before = app
        .home_selection(oc_core::queries::SessionSelectionAction::Current)
        .await
        .unwrap();
    let error = app
        .home_selection(oc_core::queries::SessionSelectionAction::Model(format!(
            "https://user:{CANARY}@private.invalid/{CANARY}\u{1b}[31m"
        )))
        .await
        .unwrap_err();
    let CoreError::Diagnostic(diagnostic) = error else {
        panic!("model refusal lost the safe owner cause");
    };
    assert_eq!(diagnostic.code, ServiceCode::ModelUnavailable);
    assert_eq!(diagnostic.stage, ServiceStage::Admission);
    assert_eq!(diagnostic.field, ["model"]);
    Fixture::safe(&diagnostic);
    conn.execute_batch(&format!("CREATE TRIGGER cfg10_refuse_selection BEFORE INSERT ON prefs BEGIN SELECT RAISE(ABORT, '{CANARY}'); END;")).unwrap();
    let error = app
        .home_selection(oc_core::queries::SessionSelectionAction::Model(
            "main".into(),
        ))
        .await
        .unwrap_err();
    let CoreError::Diagnostic(diagnostic) = error else {
        panic!("selection write lost the safe storage cause");
    };
    assert_eq!(diagnostic.kind, ServiceKind::Storage);
    assert_eq!(diagnostic.stage, ServiceStage::Storage);
    assert_eq!(diagnostic.code, ServiceCode::StorageUnavailable);
    assert_eq!(diagnostic.action, ServiceAction::ReviewStorage);
    Fixture::safe(&diagnostic);
    assert_eq!(
        app.home_selection(oc_core::queries::SessionSelectionAction::Current)
            .await
            .unwrap(),
        before
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn cfg10_title_cleanup_joins_owned_work_and_preserves_failure() {
    let task = tokio::spawn(async { panic!("owned CFG10 title worker failure") });
    while !task.is_finished() {
        tokio::task::yield_now().await;
    }
    let work = Mutex::new(AutomaticTitles {
        tasks: vec![("owned".into(), task)],
        pending: BTreeMap::new(),
    });
    let diagnostic = stop_automatic_titles(&work).await.unwrap_err();
    assert_eq!(diagnostic.kind, ServiceKind::Runtime);
    assert_eq!(diagnostic.stage, ServiceStage::Cleanup);
    assert_eq!(diagnostic.code, ServiceCode::CleanupFailed);
    assert_eq!(diagnostic.action, ServiceAction::RestartApplication);
    assert!(work.lock().unwrap().tasks.is_empty());
    Fixture::safe(&diagnostic);
}

#[tokio::test]
async fn cfg10_fatal_trust_storage_recovery_and_capacity_have_safe_typed_causes() {
    let fixture = Fixture::new();
    let lock = Db::open(&fixture.data).unwrap();
    let error = spawn_stages(&fixture.project, &fixture.data, fixture.env.clone(), false)
        .await
        .err()
        .expect("exclusive data root");
    assert_eq!(error.category, SpawnFailure::DataRootBusy);
    assert_eq!(error.diagnostic.code, ServiceCode::DataRootBusy);
    assert_eq!(error.diagnostic.stage, ServiceStage::Storage);
    assert_eq!(error.diagnostic.action, ServiceAction::CloseOtherOwner);
    Fixture::safe(&error.diagnostic);
    drop(lock);
    let db = rusqlite::Connection::open(fixture.data.join("oc.sqlite")).unwrap();
    db.execute_batch(&format!("CREATE TRIGGER cfg10_refuse_recovery BEFORE UPDATE ON tool_operations BEGIN SELECT RAISE(ABORT, '{CANARY}'); END;")).unwrap();
    drop(db);
    // Trigger refusal occurs even when the UPDATE visits no rows only with a row.
    let db = Db::open(&fixture.data).unwrap();
    db.create_session("cfg10-recovery").unwrap();
    drop(db);
    let conn = rusqlite::Connection::open(fixture.data.join("oc.sqlite")).unwrap();
    conn.execute("INSERT INTO tool_operations(id,session_id,name,state,input) VALUES ('cfg10-op','cfg10-recovery','read','started','{}')", []).unwrap();
    drop(conn);
    let error = spawn_stages(&fixture.project, &fixture.data, fixture.env.clone(), false)
        .await
        .err()
        .expect("fatal recovery");
    assert_eq!(error.category, SpawnFailure::Recovery);
    assert_eq!(error.diagnostic.code, ServiceCode::RecoveryFailed);
    assert_eq!(error.diagnostic.stage, ServiceStage::Recovery);
    assert_eq!(error.diagnostic.action, ServiceAction::ReviewRecovery);
    Fixture::safe(&error.diagnostic);
    let conn = rusqlite::Connection::open(fixture.data.join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT state FROM tool_operations WHERE id='cfg10-op'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "started"
    );
    drop(conn);
    let outside = fixture._root.path().join(format!("{CANARY}-outside"));
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.project.join(".opencode")).unwrap();
    let error = composition::load_with_env_diagnostic(&fixture.project, fixture.env.clone())
        .await
        .err()
        .expect("fatal trust");
    let composition::LoadFailure::Configuration(diagnostic) = error;
    assert_eq!(diagnostic.code, ServiceCode::TrustRefused);
    assert_eq!(diagnostic.stage, ServiceStage::Admission);
    Fixture::safe(&diagnostic);
    std::fs::remove_file(fixture.project.join(".opencode")).unwrap();
    let config = fixture.project.join("opencode.jsonc");
    std::fs::File::create(&config)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    let error = composition::load_with_env_diagnostic(&fixture.project, fixture.env)
        .await
        .err()
        .expect("fatal cap");
    let composition::LoadFailure::Configuration(diagnostic) = error;
    assert_eq!(diagnostic.code, ServiceCode::CapacityExceeded);
    assert_eq!(diagnostic.action, ServiceAction::ReduceCapacity);
    Fixture::safe(&diagnostic);
}
