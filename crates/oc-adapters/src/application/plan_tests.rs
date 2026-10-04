use super::*;
use oc_core::queries::SessionSelectionAction as Action;

#[tokio::test]
async fn plan_scoped_selection_reminder_atomic_failure_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let home = root.path().join("home");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&home).unwrap();
    std::fs::write(project.join("opencode.json"),serde_json::json!({"model":"fixture/family/model","agents":{"plan":{"system":"custom system"}},"provider":{"fixture":{"options":{"baseURL":"http://127.0.0.1:1/v1","apiKey":"synthetic"},"models":{"family/model":{}}}}}).to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let session = SessionId("plan-scope".into());
    app.create_session(session.clone()).await.unwrap();
    app.session_selection(session.clone(), false, Action::Agent("plan".into()))
        .await
        .unwrap();
    let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    let rows = || {
        conn.prepare("SELECT id,role,text FROM messages ORDER BY seq")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let original = rows();
    assert_eq!(original.len(), 1);
    assert!(original[0].2.contains("You are in Plan mode."));
    app.session_selection(session.clone(), false, Action::Agent("plan".into()))
        .await
        .unwrap();
    app.session_selection(session.clone(), false, Action::Model("family/model".into()))
        .await
        .unwrap();
    assert_eq!(rows(), original);
    assert!(!home.join(".opencode/plan").exists());
    conn.execute_batch("CREATE TRIGGER refuse_plan_leave BEFORE INSERT ON messages WHEN NEW.role='system' BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(
        app.session_selection(session.clone(), false, Action::Agent("build".into()))
            .await
            .is_err()
    );
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap()
            .agent_id
            .as_deref(),
        Some("plan")
    );
    assert_eq!(rows(), original);
    conn.execute_batch("DROP TRIGGER refuse_plan_leave")
        .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    app.create_session(session.clone()).await.unwrap();
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap()
            .agent_id
            .as_deref(),
        Some("plan")
    );
    app.session_selection(session.clone(), false, Action::Agent("build".into()))
        .await
        .unwrap();
    let after = rows();
    assert_eq!(&after[..1], original.as_slice());
    assert_eq!(after.len(), 2);
    assert_eq!(after[1].2, crate::plan::LEAVE);
    app.select_agent("plan".into()).await.unwrap();
    let legacy = rows();
    assert_eq!(legacy.len(), 3);
    assert!(legacy[2].2.contains("You are in Plan mode."));
    app.select_agent("plan".into()).await.unwrap();
    assert_eq!(rows(), legacy);
    app.select_agent("build".into()).await.unwrap();
    assert_eq!(rows().last().unwrap().2, crate::plan::LEAVE);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM turn_acceptances", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
