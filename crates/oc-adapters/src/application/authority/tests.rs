use super::*;
use crate::application::dcp_controls_tests::{config, env, request, respond};
use crate::runtime::origin::{Kind, Origin};
use oc_core::queries::SessionSelectionAction;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn srv03_idle_location_retires_only_after_last_operation_reference() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        config("https://example.invalid/v1").to_string(),
    )
    .unwrap();
    let owner = Owner::new(
        Db::open(&root.path().join("data")).unwrap(),
        env(root.path()),
    )
    .unwrap();
    let location = owner.location(&project, true).await.unwrap();
    let retained = Arc::new(Origin::new(Kind::Interactive, false).at(location.clone()));
    drop(location);
    owner.retire_idle().await.unwrap();
    assert_eq!(
        owner.loaded_locations().await.len(),
        1,
        "viewer-independent operation keeps its original Source/MCP owner"
    );
    let retired = owner.resources.retired.notified();
    drop(retained);
    retired.await;
    owner.retire_idle().await.unwrap();
    assert!(
        owner.loaded_locations().await.is_empty(),
        "no idle Location registry/watch/MCP retention"
    );
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn srv04_independent_primary_profiles_overlap_and_same_session_reserves_before_io() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    source["agents"]["alpha"] = json!({"system":"ALPHA_PRIMARY_SENTINEL"});
    source["agents"]["beta"] = json!({"system":"BETA_PRIMARY_SENTINEL"});
    source["default_agent"] = json!("alpha");
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let gate = Arc::new(tokio::sync::Barrier::new(3));
    let (entered, mut requests) = mpsc::channel(2);
    let peer_gate = gate.clone();
    let peer = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let entered = entered.clone();
            let gate = peer_gate.clone();
            clients.spawn(async move {
                let body = request(&mut socket).await;
                entered.send(body).await.unwrap();
                gate.wait().await;
                respond(&mut socket, json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"settled"}]}])).await;
            });
        }
        while let Some(result) = clients.join_next().await {
            result.unwrap();
        }
    });
    let owner = Owner::new(Db::open(&data).unwrap(), env(root.path())).unwrap();
    let location = owner.location(&project, false).await.unwrap();
    for (session, agent) in [("D", "alpha"), ("S", "beta")] {
        location
            .app
            .create_session(SessionId(session.into()))
            .await
            .unwrap();
        location
            .app
            .session_selection(
                SessionId(session.into()),
                false,
                SessionSelectionAction::Agent(agent.into()),
            )
            .await
            .unwrap();
    }
    let d = owner
        .begin(
            location.clone(),
            "D".into(),
            Arc::new(Origin::new(Kind::Run, false)),
            0,
            false,
            None,
        )
        .unwrap();
    assert!(matches!(
        owner.begin(
            location.clone(),
            "D".into(),
            Arc::new(Origin::new(Kind::Run, false)),
            0,
            false,
            None
        ),
        Err(CoreError::TurnBusy)
    ));
    let s = owner
        .begin(
            location,
            "S".into(),
            Arc::new(Origin::new(Kind::Run, false)),
            0,
            false,
            None,
        )
        .unwrap();
    let (first, second) = tokio::join!(
        d.app.submit(SessionId("D".into()), "D prompt".into()),
        s.app.submit(SessionId("S".into()), "S prompt".into())
    );
    assert!(first.is_ok() && second.is_ok(), "{first:?} {second:?}");
    let (first, second) = tokio::time::timeout(Duration::from_secs(5), async {
        (
            requests.recv().await.unwrap(),
            requests.recv().await.unwrap(),
        )
    })
    .await
    .expect("both requests must enter before either response is released");
    for body in [first, second] {
        let input = body["input"].to_string();
        if input.contains("D prompt") {
            assert!(
                input.contains("ALPHA_PRIMARY_SENTINEL")
                    && !input.contains("BETA_PRIMARY_SENTINEL")
            );
        } else {
            assert!(
                input.contains("S prompt")
                    && input.contains("BETA_PRIMARY_SENTINEL")
                    && !input.contains("ALPHA_PRIMARY_SENTINEL")
            );
        }
    }
    assert!(matches!(Db::open(&data), Err(StorageError::DataRootBusy)));
    gate.wait().await;
    peer.await.unwrap();
    owner.next_root().await.unwrap();
    owner.next_root().await.unwrap();
    assert!(owner.root("D").is_none() && owner.root("S").is_none());
    owner.shutdown().await.unwrap();
    drop(owner);
    Db::open(&data).unwrap();
}
