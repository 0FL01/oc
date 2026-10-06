use super::*;
use oc_core::queries::{TerminalAction, TerminalEntry, TerminalReceipt, TerminalSize};
use std::time::Duration;

async fn inventory(app: &CoreApp, session: &SessionId) -> oc_core::queries::TerminalInventory {
    let TerminalReceipt::Inventory(value) = app
        .terminal(session.clone(), TerminalAction::List)
        .await
        .unwrap()
    else {
        panic!("inventory receipt");
    };
    value
}
async fn create(app: &CoreApp, session: &SessionId) -> TerminalEntry {
    let catalog = app.catalog().await.unwrap();
    let TerminalReceipt::Created(value) = app
        .terminal(
            session.clone(),
            TerminalAction::Create {
                location: catalog.chrome.location.unwrap(),
                generation: catalog.chrome.selection_generation,
                size: TerminalSize::default(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("create receipt");
    };
    value
}
fn exists(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[tokio::test]
async fn term01_application_scopes_controls_and_keeps_original_pty_across_location_reload() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    let data = root.path().join("data");
    for path in [&a, &b] {
        std::fs::create_dir(path).unwrap();
        std::fs::write(
            path.join("opencode.json"),
            r#"{"disabled_providers":["opencode-go"]}"#,
        )
        .unwrap();
    }
    let (app, guard, _) = spawn_with_env(
        &a,
        &data,
        BTreeMap::from([
            ("SHELL".into(), "/bin/bash".into()),
            ("HOME".into(), a.display().to_string()),
            (
                "OC_API_KEY".into(),
                "TERM01_SYNTHETIC_MUST_NOT_INHERIT".into(),
            ),
        ]),
    )
    .await
    .unwrap();
    let source = SessionId("terminal-source".into());
    let other = SessionId("terminal-other".into());
    app.create_session(source.clone()).await.unwrap();
    app.create_session(other.clone()).await.unwrap();
    assert!(app.catalog().await.unwrap().chrome.session_terminal);
    assert!(inventory(&app, &source).await.enabled);
    assert!(inventory(&app, &source).await.entries.is_empty()); // no startup shell
    let first = create(&app, &source).await;
    let second = create(&app, &source).await;
    app.terminal(
        source.clone(),
        TerminalAction::Select(Some(first.target.clone())),
    )
    .await
    .unwrap();
    assert!(
        app.terminal(
            other.clone(),
            TerminalAction::Input {
                target: first.target.clone(),
                bytes: b"echo foreign\n".to_vec()
            }
        )
        .await
        .is_err()
    );
    let mut stale = first.target.clone();
    stale.generation += 1;
    assert!(
        app.terminal(source.clone(), TerminalAction::Snapshot(stale))
            .await
            .is_err()
    );
    assert!(app.delete_session(source.clone()).await.is_err()); // retain live recovery identity
    let before = app.catalog().await.unwrap().chrome.selection_generation;
    app.reload_location().await.unwrap();
    let after = app.catalog().await.unwrap().chrome.selection_generation;
    assert!(after > before);
    assert!(
        app.terminal(
            source.clone(),
            TerminalAction::Create {
                location: a.canonicalize().unwrap().display().to_string(),
                generation: before,
                size: TerminalSize::default(),
            }
        )
        .await
        .is_err()
    );
    app.switch_location_home(b.display().to_string())
        .await
        .unwrap();
    let current = inventory(&app, &source).await;
    assert_eq!(current.entries.len(), 2);
    assert_eq!(current.selected, Some(first.target.clone()));
    assert_eq!(current.entries[0].target, first.target);
    app.terminal(
        source.clone(),
        TerminalAction::Input {
            target: first.target.clone(),
            bytes: b"printf 'TER%s'; pwd; test -z \"$OC_API_KEY\" && printf 'SAFE%s' ENV\n"
                .to_vec(),
        },
    )
    .await
    .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let TerminalReceipt::Snapshot(snapshot) = app
            .terminal(
                source.clone(),
                TerminalAction::Snapshot(first.target.clone()),
            )
            .await
            .unwrap()
        else {
            panic!("snapshot");
        };
        let text: String = snapshot.cells.iter().map(|c| c.text.as_str()).collect();
        if text.contains("SAFEENV") {
            assert!(text.contains(&a.display().to_string()));
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "actual PTY output");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    app.terminal(source.clone(), TerminalAction::Select(None))
        .await
        .unwrap();
    assert!(exists(first.pid) && exists(second.pid));
    let mut events = app.subscribe();
    app.terminal(source.clone(), TerminalAction::Remove(first.target))
        .await
        .unwrap();
    assert!(!exists(first.pid));
    assert!(
        matches!(events.try_recv(), Ok(CoreEvent::TerminalChanged { session }) if session == source)
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(!exists(second.pid));
    let db = Db::open(&data).unwrap();
    assert!(db.selected_terminal(&source.0).unwrap().is_none());
    assert!(db.list_tool_ops(&source.0).unwrap().is_empty());
    drop(db);
    let sql = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    assert_eq!(
        sql.query_row("SELECT count(*) FROM terminals WHERE live=1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        sql.query_row(
            "SELECT applied_at FROM schema_migrations WHERE version=12",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "t56-terminals"
    );
    assert_eq!(
        sql.query_row(
            "SELECT applied_at FROM schema_migrations WHERE version=11",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "t53-credentials"
    );
}

#[tokio::test]
async fn term01_terminal_recovery_failure_keeps_local_application_but_not_fake_success() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        r#"{"disabled_providers":["opencode-go"]}"#,
    )
    .unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("source", &project.display().to_string())
            .unwrap();
    }
    {
        let sql = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        sql.execute("INSERT INTO terminals(id,session_id,entry,process,live) VALUES('invalid','source','{}','{}',1)", []).unwrap();
    }
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(app.list_sessions().await.unwrap().len(), 1);
    assert!(
        app.terminal(SessionId("source".into()), TerminalAction::List)
            .await
            .is_err()
    );
    assert!(app.catalog().await.is_ok());
    app.shutdown().await.unwrap();
    assert!(guard.join().await.is_err()); // failed recovery/cleanup never a successful shutdown
}

#[tokio::test]
async fn term01_scripted_core_does_not_masquerade_as_native_terminal_owner() {
    let (app, guard) = CoreApp::spawn(oc_core::core_app::MockProvider::echo());
    assert!(
        app.terminal(SessionId("source".into()), TerminalAction::List)
            .await
            .is_err()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
