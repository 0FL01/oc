//! Command selection must be validated before accepting input or writing prefs.
use super::*;
use oc_core::queries::SessionSelectionAction as Action;

#[tokio::test]
async fn command_routing_invalid_metadata_has_no_acceptance_or_selection_effects() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model":"fixture/m", "permission":{"subagent":"allow"},
        "provider":{"fixture":{"options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"dummy"},"models":{"m":{}}}},
        "agent":{"other":{"mode":"primary","prompt":"OTHER"}},
        "command":{
            "missing":{"template":"expanded $1","agent":"absent"},
            "badmodel":{"template":"expanded $1","agent":"other","model":"fixture/absent"}
        }
    }).to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let session = SessionId("command-invalid".into());
    app.create_session(session.clone()).await.unwrap();
    let before = app
        .session_selection(session.clone(), false, Action::Current)
        .await
        .unwrap();
    for invocation in ["/missing literal", "/badmodel literal"] {
        assert!(
            app.submit(session.clone(), invocation.into())
                .await
                .is_err(),
            "{invocation} accepted"
        );
        assert!(app.read_history(session.clone()).await.unwrap().is_empty());
        assert_eq!(
            app.session_selection(session.clone(), false, Action::Current)
                .await
                .unwrap(),
            before
        );
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
