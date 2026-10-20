use super::*;
use crate::application::dcp_controls_tests::{config, env, request, respond};
use crate::runtime::origin::{Kind, Origin};
use oc_core::queries::SessionSelectionAction;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn idle_root_child_report_starts_one_parent_request_with_current_selection() {
    parent_notice_episode(ParentEpisode::CurrentSelection).await;
}

#[tokio::test]
async fn parent_notice_defers_until_actual_mutation_release() {
    parent_notice_episode(ParentEpisode::MutationRelease).await;
}

#[tokio::test]
async fn last_tui_suppresses_completed_parent_handoff_before_origin_settlement() {
    parent_notice_episode(ParentEpisode::InteractiveRelease).await;
}

#[tokio::test]
async fn run_release_suppresses_completed_parent_handoff_before_settlement() {
    parent_notice_episode(ParentEpisode::RunRelease).await;
}

#[tokio::test]
async fn independent_run_parent_handoff_survives_last_tui() {
    parent_notice_episode(ParentEpisode::RunSurvivesTui).await;
}

#[tokio::test]
async fn idle_session_cancel_suppresses_pending_handoff_under_root_admission_lock() {
    parent_notice_episode(ParentEpisode::SessionCancel).await;
}

#[tokio::test]
async fn permanent_native_admission_refusal_is_visible_once_without_requeue() {
    parent_notice_episode(ParentEpisode::PermanentRefusal).await;
}

#[tokio::test]
async fn failed_child_report_wakes_its_idle_root_without_replaying_child() {
    parent_notice_episode(ParentEpisode::ChildError).await;
}

#[tokio::test]
async fn individually_interrupted_child_still_reports_to_its_live_parent() {
    parent_notice_episode(ParentEpisode::ChildInterrupted).await;
}

#[tokio::test]
async fn foreground_conversion_keeps_original_parent_wake_lineage() {
    parent_notice_episode(ParentEpisode::ConvertedForeground).await;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParentEpisode {
    CurrentSelection,
    MutationRelease,
    InteractiveRelease,
    RunRelease,
    RunSurvivesTui,
    SessionCancel,
    PermanentRefusal,
    ChildError,
    ChildInterrupted,
    ConvertedForeground,
}

async fn parent_notice_episode(case: ParentEpisode) {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    source["agents"]["alpha"] = json!({"system":"ORIGINAL_PARENT_PROFILE"});
    source["agents"]["beta"] =
        json!({"system":"CURRENT_PARENT_PROFILE","model":"fixture/beta-model"});
    source["agents"]["helper"] =
        json!({"mode":"subagent","system":"OWN_CHILD_PROFILE","model":"fixture/agent-model"});
    source["default_agent"] = json!("alpha");
    for model in ["agent-model", "beta-model"] {
        source["providers"]["fixture"]["models"][model] =
            json!({"limit":{"context":100000,"output":2048}});
    }
    if case == ParentEpisode::PermanentRefusal {
        source["providers"]["fixture"]["models"]["beta-model"]["limit"]["context"] = json!(32);
    }
    source["permissions"] = json!([{"action":"subagent","resource":"*","effect":"allow"}]);
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let release = Arc::new(tokio::sync::Notify::new());
    let child_ready = Arc::new(tokio::sync::Notify::new());
    let parent_posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let child_posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (entered, mut requests) = mpsc::channel(4);
    let peer = {
        let release = release.clone();
        let child_ready = child_ready.clone();
        let parent_posts = parent_posts.clone();
        let child_posts = child_posts.clone();
        tokio::spawn(async move {
            let mut clients = tokio::task::JoinSet::new();
            for _ in 0..4 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let entered = entered.clone();
                let release = release.clone();
                let child_ready = child_ready.clone();
                let parent_posts = parent_posts.clone();
                let child_posts = child_posts.clone();
                clients.spawn(async move {
                    let body = request(&mut socket).await;
                    let child = body["model"] == "agent-model";
                    entered.send(body).await.unwrap();
                    let output = if child {
                        child_posts.fetch_add(1, Ordering::AcqRel);
                        child_ready.notify_one();
                        release.notified().await;
                        if case == ParentEpisode::ChildError {
                            use tokio::io::AsyncWriteExt;
                            socket.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                            return;
                        }
                        json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"CHILD_REPORT_SENTINEL"}]}])
                    } else if parent_posts.fetch_add(1, Ordering::AcqRel) == 0 {
                        json!([{"type":"function_call","id":"bg-item","call_id":"bg-launch","name":"subagent","arguments":json!({"agent":"helper","description":"independent investigation","prompt":"OWN_CHILD_TASK","background":case != ParentEpisode::ConvertedForeground}).to_string()}])
                    } else {
                        json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"PARENT_SETTLED"}]}])
                    };
                    respond(&mut socket, output).await;
                });
            }
            while let Some(result) = clients.join_next().await {
                result.unwrap();
            }
        })
    };
    let owner = Owner::new(
        Db::open(&root.path().join("data")).unwrap(),
        env(root.path()),
    )
    .unwrap();
    let location = owner.location(&project, false).await.unwrap();
    let session = SessionId("parent".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "Named parent".into())
        .await
        .unwrap();
    let kind = if matches!(
        case,
        ParentEpisode::RunRelease | ParentEpisode::RunSurvivesTui
    ) {
        Kind::Run
    } else {
        Kind::Interactive
    };
    let origin = Arc::new(Origin::new(kind, false).at(location.clone()));
    let running = owner
        .begin(
            location.clone(),
            session.0.clone(),
            origin.clone(),
            0,
            false,
            None,
        )
        .unwrap();
    let mut events = owner.events.subscribe();
    let first = running
        .app
        .submit(session.clone(), "ORIGINAL_PARENT_TASK".into())
        .await
        .unwrap();
    if case == ParentEpisode::ConvertedForeground {
        tokio::time::timeout(Duration::from_secs(5), child_ready.notified())
            .await
            .unwrap();
        let job = owner.db.child_tool_jobs("parent").unwrap().remove(0);
        assert!(!job.background);
        assert!(owner.resources.children.background("parent", &job));
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if matches!(event, CoreEvent::TurnFinished{turn,..} if turn == first) {
                break;
            }
        }
    })
    .await
    .unwrap();
    owner.next_root().await.unwrap();
    assert!(
        owner.root("parent").is_none(),
        "original root must be fully idle before child completion"
    );
    drop(running);
    assert_eq!(parent_posts.load(Ordering::Acquire), 2);
    assert_eq!(child_posts.load(Ordering::Acquire), 1);
    origin.auto_once.store(true, Ordering::Release);
    let mut blocked = if matches!(
        case,
        ParentEpisode::MutationRelease
            | ParentEpisode::InteractiveRelease
            | ParentEpisode::RunRelease
            | ParentEpisode::SessionCancel
            | ParentEpisode::PermanentRefusal
    ) {
        Some(owner.reserve_mutation("parent").unwrap())
    } else {
        None
    };
    location
        .app
        .session_selection(
            session.clone(),
            false,
            SessionSelectionAction::Agent("beta".into()),
        )
        .await
        .unwrap();
    if case == ParentEpisode::RunSurvivesTui {
        owner.cancel_interactive();
    }
    if case == ParentEpisode::ChildInterrupted {
        let job = owner.db.child_tool_jobs("parent").unwrap().remove(0);
        assert!(owner.resources.children.interrupt("parent", &job));
        assert!(!origin.foreground.load(Ordering::Acquire));
        // Keep the peer held: cancellation must settle the child without a
        // response, and must not cancel the original parent Origin.
    } else {
        release.notify_one();
    }
    let mut seen = Vec::new();
    let resumed = tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..3 { seen.push(requests.recv().await.unwrap()); }
        if blocked.is_some() {
            loop {
                let event = match events.recv().await.unwrap() { CoreEvent::Projection{event,..} => *event, event => event };
                if matches!(event,CoreEvent::ChildNotice(_)) { break; }
            }
            assert!(owner.root("parent").is_none(),"mutation must defer admission");
        }
        match case {
            ParentEpisode::InteractiveRelease => owner.cancel_interactive(),
            ParentEpisode::RunRelease => owner.cancel_origin(&origin),
            ParentEpisode::SessionCancel => owner.cancel_session(&location,"parent",None).unwrap(),
            _ => {},
        }
        drop(blocked.take());
        if matches!(case,ParentEpisode::InteractiveRelease|ParentEpisode::RunRelease|ParentEpisode::SessionCancel) {
            origin.settle().await;
            assert!(owner.root("parent").is_none());
            return;
        }
        if case == ParentEpisode::PermanentRefusal {
            loop {
                let event = match events.recv().await.unwrap() { CoreEvent::Projection{event,..} => *event, event => event };
                if let CoreEvent::ParentContinuationRefused { session:target,diagnostic } = event {
                    assert_eq!(target,session);
                    assert!(!diagnostic.to_string().contains("CHILD_REPORT_SENTINEL"));
                    break;
                }
            }
            owner.next_root().await.unwrap();
            return;
        }
        seen.push(requests.recv().await.unwrap());
        loop {
            let event = match events.recv().await.unwrap() { CoreEvent::Projection{event,..} => *event, event => event };
            if matches!(event, CoreEvent::TurnFinished{session:ref target,turn,..} if target == &session && turn != first) { break; }
        }
        owner.next_root().await.unwrap();
    }).await;
    owner.shutdown().await.unwrap();
    loop {
        let event = match events.try_recv() {
            Ok(CoreEvent::Projection { event, .. }) => *event,
            Ok(event) => event,
            Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed) => {
                break;
            }
            Err(error) => panic!("lost continuation disposition: {error}"),
        };
        assert!(
            !matches!(event, CoreEvent::ParentContinuationRefused { .. }),
            "duplicate or unexpected native continuation refusal"
        );
    }
    peer.abort();
    let _ = peer.await;
    assert!(
        resumed.is_ok(),
        "native parent episode {case:?} did not settle; parents={}, children={}, physical={:?}",
        parent_posts.load(Ordering::Acquire),
        child_posts.load(Ordering::Acquire),
        seen.iter()
            .map(|body| (
                &body["model"],
                body["input"].to_string().contains("ORIGINAL_PARENT_TASK"),
                body["input"].to_string().contains("CHILD_REPORT_SENTINEL")
            ))
            .collect::<Vec<_>>()
    );
    assert!(
        origin.auto_once.load(Ordering::Acquire),
        "automatic startup must not reset original authority flags"
    );
    assert_eq!(
        owner.origins.lock().unwrap().len(),
        1,
        "same origin registration is deduplicated"
    );
    let suppressed = matches!(
        case,
        ParentEpisode::InteractiveRelease
            | ParentEpisode::RunRelease
            | ParentEpisode::SessionCancel
            | ParentEpisode::PermanentRefusal
    );
    if suppressed {
        assert_eq!(parent_posts.load(Ordering::Acquire), 2);
        assert_eq!(child_posts.load(Ordering::Acquire), 1);
        let jobs = owner.db.child_tool_jobs("parent").unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(
            !owner
                .db
                .parent_notice_assigned("parent", jobs[0].message_id.as_deref().unwrap())
                .unwrap()
        );
        assert_eq!(
            owner
                .db
                .read_history_full("parent")
                .unwrap()
                .iter()
                .filter(|(_, role, text)| role == "user" && text.is_empty())
                .count(),
            0
        );
        return;
    }
    assert_eq!(parent_posts.load(Ordering::Acquire), 3);
    assert_eq!(child_posts.load(Ordering::Acquire), 1);
    let automatic = seen
        .iter()
        .find(|body| body["model"] == "beta-model")
        .expect("committed parent selection, not child or original model");
    let input = automatic["input"].to_string();
    assert!(
        (matches!(
            case,
            ParentEpisode::ChildError | ParentEpisode::ChildInterrupted
        ) || input.contains("CHILD_REPORT_SENTINEL"))
            && input.contains("ORIGINAL_PARENT_TASK")
            && input.contains("CURRENT_PARENT_PROFILE"),
        "{input}"
    );
    if !matches!(
        case,
        ParentEpisode::ChildError | ParentEpisode::ChildInterrupted
    ) {
        assert_eq!(input.matches("CHILD_REPORT_SENTINEL").count(), 1);
    }
    let jobs = owner.db.child_tool_jobs("parent").unwrap();
    assert_eq!(jobs.len(), 1);
    if case == ParentEpisode::ChildInterrupted {
        assert_eq!(jobs[0].state, oc_core::queries::ChildState::Cancelled);
    }
    if case == ParentEpisode::ConvertedForeground {
        assert!(jobs[0].background);
    }
    assert!(
        owner
            .db
            .parent_notice_assigned("parent", jobs[0].message_id.as_deref().unwrap())
            .unwrap()
    );
    assert_eq!(
        owner
            .db
            .read_history_full("parent")
            .unwrap()
            .iter()
            .filter(|(_, role, text)| role == "user" && text.is_empty())
            .count(),
        1
    );
}

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
