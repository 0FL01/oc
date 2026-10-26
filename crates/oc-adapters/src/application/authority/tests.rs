use super::*;
use crate::application::dcp_controls_tests::{config, env, request, respond};
use crate::runtime::origin::{Kind, Origin};
use oc_core::queries::{ConversationAction, SessionSelectionAction};
use serde_json::json;
use std::time::Duration;
mod shell;

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

#[tokio::test]
async fn active_undo_holds_child_report_until_redo_without_replaying_child() {
    parent_notice_episode(ParentEpisode::UndoRedo).await;
}

#[tokio::test]
async fn undo_new_prompt_discards_already_queued_child_report() {
    parent_notice_episode(ParentEpisode::UndoCommit).await;
}

#[tokio::test]
async fn undo_new_prompt_keeps_child_that_completes_after_branch_commit() {
    parent_notice_episode(ParentEpisode::UndoLate).await;
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
    UndoRedo,
    UndoCommit,
    UndoLate,
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
            for _ in 0..5 {
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
                        if case == ParentEpisode::UndoRedo
                            && parent_posts.load(Ordering::Acquire) == 2
                        {
                            std::future::pending::<()>().await;
                        }
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
    let mut seen = Vec::new();
    if case == ParentEpisode::UndoRedo {
        tokio::time::timeout(Duration::from_secs(5), async {
            for _ in 0..3 {
                seen.push(requests.recv().await.unwrap());
            }
            let undone = running
                .app
                .change_conversation(session.clone(), ConversationAction::Undo)
                .await
                .unwrap();
            assert!(undone.can_redo);
            assert_eq!(undone.draft.as_deref(), Some("ORIGINAL_PARENT_TASK"));
        })
        .await
        .unwrap();
        assert!(origin.foreground.load(Ordering::Acquire));
    }
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
            if match event {
                CoreEvent::TurnInterrupted { turn, .. } if case == ParentEpisode::UndoRedo => {
                    turn == first
                }
                CoreEvent::TurnFinished { turn, .. } if case != ParentEpisode::UndoRedo => {
                    turn == first
                }
                _ => false,
            } {
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
    assert!(matches!(
        owner.db.child_tool_jobs("parent").unwrap()[0].state,
        oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
    ));
    origin.auto_once.store(true, Ordering::Release);
    let mut blocked = if matches!(
        case,
        ParentEpisode::MutationRelease
            | ParentEpisode::InteractiveRelease
            | ParentEpisode::RunRelease
            | ParentEpisode::SessionCancel
            | ParentEpisode::PermanentRefusal
            | ParentEpisode::UndoRedo
            | ParentEpisode::UndoCommit
            | ParentEpisode::UndoLate
    ) {
        Some(owner.reserve_mutation("parent").unwrap())
    } else {
        None
    };
    if matches!(case, ParentEpisode::UndoCommit | ParentEpisode::UndoLate) {
        location
            .app
            .change_conversation(session.clone(), ConversationAction::Undo)
            .await
            .unwrap();
    }
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
    let resumed = tokio::time::timeout(Duration::from_secs(5), async {
        while seen.len() < 3 { seen.push(requests.recv().await.unwrap()); }
        if case == ParentEpisode::UndoLate {
            drop(blocked.take());
            seen.push(replace_parent(&owner, &location, &session, &mut events, &mut requests).await);
            blocked = Some(owner.reserve_mutation("parent").unwrap());
        }
        if case == ParentEpisode::ChildInterrupted {
            let job = owner.db.child_tool_jobs("parent").unwrap().remove(0);
            assert!(owner.resources.children.interrupt("parent", &job));
            assert!(!origin.foreground.load(Ordering::Acquire));
        } else {
            release.notify_one();
        }
        if blocked.is_some() {
            loop {
                let event = match events.recv().await.unwrap() { CoreEvent::Projection{event,..} => *event, event => event };
                if matches!(event,CoreEvent::ChildNotice(_)) { break; }
            }
            assert!(owner.root("parent").is_none(),"mutation must defer admission");
        }
        if matches!(case, ParentEpisode::UndoRedo | ParentEpisode::UndoCommit) {
            assert!(owner.resources.children.parent_batches().unwrap().is_empty());
            drop(blocked.take());
            owner.reconcile_parent_notices().unwrap();
            assert!(owner.root("parent").is_none(), "staged Undo must not start or refuse a continuation");
            assert_eq!(parent_posts.load(Ordering::Acquire), 2);
            if case == ParentEpisode::UndoRedo {
                blocked = Some(owner.reserve_mutation("parent").unwrap());
                let redo = location.app.change_conversation(session.clone(), ConversationAction::Redo).await.unwrap();
                assert!(!redo.can_redo && redo.reverted.is_none());
            } else {
                seen.push(replace_parent(&owner, &location, &session, &mut events, &mut requests).await);
                owner.resources.children.parent_batches().unwrap();
                origin.settle().await;
                assert!(owner.root("parent").is_none());
                return;
            }
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
        if case == ParentEpisode::UndoCommit {
            2
        } else {
            1
        },
        "same origin registration is deduplicated"
    );
    let suppressed = matches!(
        case,
        ParentEpisode::InteractiveRelease
            | ParentEpisode::RunRelease
            | ParentEpisode::SessionCancel
            | ParentEpisode::PermanentRefusal
            | ParentEpisode::UndoCommit
    );
    if suppressed {
        assert_eq!(
            parent_posts.load(Ordering::Acquire),
            if case == ParentEpisode::UndoCommit {
                3
            } else {
                2
            }
        );
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
        if case == ParentEpisode::UndoCommit {
            let input = seen.last().unwrap()["input"].to_string();
            assert!(input.contains("REPLACEMENT_TASK"));
            assert!(
                !input.contains("CHILD_REPORT_SENTINEL") && !input.contains("ORIGINAL_PARENT_TASK")
            );
        }
        return;
    }
    assert_eq!(
        parent_posts.load(Ordering::Acquire),
        if case == ParentEpisode::UndoLate {
            4
        } else {
            3
        }
    );
    assert_eq!(child_posts.load(Ordering::Acquire), 1);
    let automatic = seen.last().unwrap();
    assert_eq!(automatic["model"], "beta-model");
    let input = automatic["input"].to_string();
    assert!(
        (matches!(
            case,
            ParentEpisode::ChildError | ParentEpisode::ChildInterrupted
        ) || input.contains("CHILD_REPORT_SENTINEL"))
            && input.contains(if case == ParentEpisode::UndoLate {
                "REPLACEMENT_TASK"
            } else {
                "ORIGINAL_PARENT_TASK"
            })
            && input.contains("CURRENT_PARENT_PROFILE"),
        "{input}"
    );
    if case == ParentEpisode::UndoLate {
        assert!(!input.contains("ORIGINAL_PARENT_TASK"));
    }
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

/// Commit a staged branch through real native admission, before or after a
/// detached job reports. The replacement never launches another job.
async fn replace_parent(
    owner: &Arc<Owner>,
    location: &Arc<Location>,
    session: &SessionId,
    events: &mut broadcast::Receiver<CoreEvent>,
    requests: &mut mpsc::Receiver<serde_json::Value>,
) -> serde_json::Value {
    let origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
    let running = owner
        .begin(location.clone(), session.0.clone(), origin, 0, false, None)
        .unwrap();
    let turn = running
        .app
        .submit(session.clone(), "REPLACEMENT_TASK".into())
        .await
        .unwrap();
    let body = requests.recv().await.unwrap();
    loop {
        let event = match events.recv().await.unwrap() {
            CoreEvent::Projection { event, .. } => *event,
            event => event,
        };
        assert!(!matches!(
            event,
            CoreEvent::ParentContinuationRefused { .. }
        ));
        if matches!(event, CoreEvent::TurnFinished { turn: id, .. } if id == turn) {
            break;
        }
    }
    owner.next_root().await.unwrap();
    assert!(owner.root(&session.0).is_none());
    body
}

#[tokio::test]
async fn source_queries_and_sibling_primary_cannot_consume_an_owned_move() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut raw = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    raw["permissions"] = json!([
        {"action":"opencode_session_move","resource":"*","effect":"allow"},
        {"action":"external_directory","resource":"*","effect":"allow"}
    ]);
    for project in [&a, &b] {
        std::fs::write(project.join("opencode.json"), raw.to_string()).unwrap();
    }
    let sibling_ready = Arc::new(tokio::sync::Notify::new());
    let sibling_release = Arc::new(tokio::sync::Notify::new());
    let ready = sibling_ready.clone();
    let release = sibling_release.clone();
    let destination = b.to_string_lossy().into_owned();
    let peer = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        let posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let ready = ready.clone();
            let release = release.clone();
            let destination = destination.clone();
            let posts = posts.clone();
            clients.spawn(async move {
                let body = request(&mut socket).await;
                if body["input"].to_string().contains("SIBLING_TASK") {
                    ready.notify_one();
                    release.notified().await;
                    respond(&mut socket, json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"sibling settled"}]}])).await;
                } else if posts.fetch_add(1, Ordering::AcqRel) == 0 {
                    respond(&mut socket, json!([{"type":"function_call","id":"move-item","call_id":"move-call","name":"opencode_session_move","arguments":json!({"directory":destination}).to_string()}])).await;
                } else {
                    respond(&mut socket, json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"moving settled"}]}])).await;
                }
            });
        }
        while let Some(result) = clients.join_next().await {
            result.unwrap();
        }
    });
    let owner = Owner::new(
        Db::open(&root.path().join("data")).unwrap(),
        env(root.path()),
    )
    .unwrap();
    let source = owner.location(&a, false).await.unwrap();
    let target = owner.location(&b, false).await.unwrap();
    for session in ["moving", "sibling"] {
        source
            .app
            .create_session(SessionId(session.into()))
            .await
            .unwrap();
        source
            .app
            .rename_session(SessionId(session.into()), session.into())
            .await
            .unwrap();
    }
    let gate = target.deck_gate.lock().await;
    let sibling_origin = Arc::new(Origin::new(Kind::Interactive, false).at(source.clone()));
    let sibling = owner
        .begin(
            source.clone(),
            "sibling".into(),
            sibling_origin,
            2,
            false,
            None,
        )
        .unwrap();
    sibling.ready().await.unwrap();
    sibling
        .app
        .submit(SessionId("sibling".into()), "SIBLING_TASK".into())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), sibling_ready.notified())
        .await
        .unwrap();
    let origin = Arc::new(Origin::new(Kind::Interactive, false).at(source.clone()));
    let moving = owner
        .begin(source.clone(), "moving".into(), origin, 2, false, None)
        .unwrap();
    moving.ready().await.unwrap();
    let mut events = owner.events.subscribe();
    let turn = moving
        .app
        .submit(SessionId("moving".into()), "OWNED_MOVE_TASK".into())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            let event = match event {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if matches!(event, CoreEvent::TurnFinished { turn: ref id, .. } if id == &turn) {
                break;
            }
        }
    })
    .await
    .unwrap();
    // These real Source query round trips give its idle worker the opportunity
    // to scan the completed move. Only the issuing primary owns its application.
    let first_query = source.app.catalog().await;
    let second_query = source.app.catalog().await;
    let pending = owner
        .db
        .ready_session_moves(&a.to_string_lossy())
        .unwrap()
        .into_iter()
        .find(|record| record.session == "moving");
    sibling_release.notify_one();
    let sibling_retires = matches!(
        tokio::time::timeout(std::time::Duration::from_secs(5), owner.next_root()).await,
        Ok(Ok(()))
    );
    drop(gate);
    let moved = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            let event = match event {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if matches!(event, CoreEvent::SessionMoved { ref session, .. } if session.0 == "moving")
            {
                break;
            }
        }
    })
    .await
    .is_ok();
    let moving_retires = matches!(
        tokio::time::timeout(std::time::Duration::from_secs(5), owner.next_root()).await,
        Ok(Ok(()))
    );
    let stale_rejected = owner.root("moving").is_none()
        && pending
            .as_ref()
            .is_some_and(|record| matches!(owner.source_move_available(record), Ok(false)));
    let shutdown = owner.shutdown().await;
    peer.abort();
    let _ = peer.await;
    assert!(
        pending.is_some(),
        "Source applied a primary's move without its held deck gate"
    );
    assert!(
        sibling_retires,
        "an unrelated primary consumed the pending move"
    );
    assert!(first_query.is_ok() && second_query.is_ok());
    assert!(moved);
    assert!(
        moving_retires && stale_rejected,
        "a retired primary's stale ready list became Source fallback"
    );
    shutdown.unwrap();
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

#[tokio::test]
async fn busy_agent_commit_applies_from_the_next_request_of_the_running_root() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("note"), "note body").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    source["agents"]["alpha"] = json!({"system":"ALPHA_PRIMARY_SENTINEL"});
    source["agents"]["beta"] = json!({"system":"BETA_PRIMARY_SENTINEL", "model":"fixture/retired"});
    source["providers"]["fixture"]["models"]["replacement"] =
        source["providers"]["fixture"]["models"]["m"].clone();
    source["default_agent"] = json!("alpha");
    source["permissions"] = json!([{"action": "*", "resource": "*", "effect": "allow"}]);
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let (held, mut held_rx) = mpsc::channel(1);
    let (release, gate) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let mut bodies = Vec::new();
        let mut gate = Some(gate);
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            bodies.push(request(&mut socket).await);
            if step == 0 {
                held.send(()).await.unwrap();
                gate.take().unwrap().await.unwrap();
                respond(&mut socket, json!([{"type":"function_call","id":"read-item","call_id":"read-call","name":"read","arguments":"{\"path\":\"note\"}"}])).await;
            } else {
                respond(&mut socket, json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"settled"}]}])).await;
            }
        }
        bodies
    });
    let owner = Owner::new(Db::open(&data).unwrap(), env(root.path())).unwrap();
    let location = owner.location(&project, false).await.unwrap();
    let session = SessionId("R".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "fixed".into())
        .await
        .unwrap();
    // A preview resolves the other agent's choice without committing it.
    let preview = location
        .app
        .session_selection(
            session.clone(),
            false,
            SessionSelectionAction::Preview("beta".into()),
        )
        .await
        .unwrap();
    assert_eq!(preview.agent_id.as_deref(), Some("beta"));
    assert_eq!(
        preview.chrome.selection.as_ref().unwrap().diagnostic.code,
        oc_core::queries::ServiceCode::ModelUnavailable
    );
    let current = location
        .app
        .session_selection(session.clone(), false, SessionSelectionAction::Current)
        .await
        .unwrap();
    assert_eq!(current.agent_id.as_deref(), Some("alpha"));
    let run = owner
        .begin(
            location,
            "R".into(),
            Arc::new(Origin::new(Kind::Run, false)),
            0,
            false,
            None,
        )
        .unwrap();
    run.app
        .submit(session.clone(), "R prompt".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), held_rx.recv())
        .await
        .unwrap()
        .unwrap();
    // OC2 switchAgent while busy: accepted, never relabels the issued request.
    let switched = run
        .app
        .session_selection(
            session.clone(),
            false,
            SessionSelectionAction::AgentCommit(oc_core::queries::ModelCommit {
                attachment: 0,
                caller: 1,
                binding: oc_core::queries::SelectionBinding {
                    location: preview.chrome.location,
                    generation: preview.chrome.selection_generation,
                    provider: "fixture".into(),
                    agent_id: preview.agent_id,
                },
                model_id: "replacement".into(),
                variant: None,
                draft_revision: 1,
            }),
        )
        .await
        .unwrap();
    assert_eq!(switched.agent_id.as_deref(), Some("beta"));
    assert_eq!(switched.model_id, "replacement");
    release.send(()).unwrap();
    let bodies = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bodies[0]["model"], "m");
    assert_eq!(bodies[1]["model"], "replacement");
    let (issued, next) = (bodies[0].to_string(), bodies[1].to_string());
    assert!(issued.contains("ALPHA_PRIMARY_SENTINEL") && !issued.contains("BETA_PRIMARY_SENTINEL"));
    assert!(next.contains("BETA_PRIMARY_SENTINEL") && !next.contains("ALPHA_PRIMARY_SENTINEL"));
    assert!(
        next.contains("note body"),
        "the issued tool result is retained"
    );
    owner.next_root().await.unwrap();
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn steer_joins_the_running_turn_and_queue_runs_after_it_settles() {
    use oc_core::queries::InboxDelivery;
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let (held, mut held_rx) = mpsc::channel(1);
    let (release, gate) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let mut bodies = Vec::new();
        let mut gate = Some(gate);
        for step in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            bodies.push(request(&mut socket).await);
            if step == 0 {
                held.send(()).await.unwrap();
                gate.take().unwrap().await.unwrap();
            }
            // Every answer is final: only pending input continues the work.
            respond(&mut socket, json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":format!("answer {step}")}]}])).await;
        }
        bodies
    });
    let owner = Owner::new(Db::open(&data).unwrap(), env(root.path())).unwrap();
    let location = owner.location(&project, false).await.unwrap();
    let session = SessionId("R".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "fixed".into())
        .await
        .unwrap();
    // Idle sessions take an ordinary submission, not inbox input.
    assert_eq!(
        location
            .app
            .enqueue(session.clone(), "idle".into(), InboxDelivery::Steer)
            .await,
        Err(CoreError::TurnNotActive)
    );
    let run = owner
        .begin(
            location,
            "R".into(),
            Arc::new(Origin::new(Kind::Run, false)),
            0,
            false,
            None,
        )
        .unwrap();
    run.app
        .submit(session.clone(), "first prompt".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), held_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let queued = run
        .app
        .enqueue(
            session.clone(),
            "queued prompt".into(),
            InboxDelivery::Queue,
        )
        .await
        .unwrap();
    run.app
        .enqueue(session.clone(), "steer prompt".into(), InboxDelivery::Steer)
        .await
        .unwrap();
    let pending = run
        .app
        .inbox(session.clone(), oc_core::queries::InboxAction::List)
        .await
        .unwrap();
    assert_eq!(pending.items.len(), 2);
    assert_eq!(pending.items[0], queued);
    release.send(()).unwrap();
    let bodies = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap();
    let users = |body: &serde_json::Value| {
        body["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["role"] == "user")
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    // Request 2 continues turn 1 after its final answer, with the steer last.
    let second = users(&bodies[1]);
    assert!(second.last().unwrap().contains("steer prompt"));
    assert!(bodies[1].to_string().contains("answer 0"));
    assert!(!bodies[1].to_string().contains("queued prompt"));
    // Request 3 is the queued input's own turn, after the settled turn 1.
    let third = users(&bodies[2]);
    assert!(third.last().unwrap().contains("queued prompt"));
    assert!(bodies[2].to_string().contains("answer 1"));
    owner.next_root().await.unwrap();
    owner.shutdown().await.unwrap();
    drop(owner);
    let history = Db::open(&data).unwrap().read_history("R").unwrap();
    assert_eq!(
        history,
        [
            ("user".into(), "first prompt".into()),
            ("user".into(), "steer prompt".into()),
            ("assistant".into(), "answer 0answer 1".into()),
            ("user".into(), "queued prompt".into()),
            ("assistant".into(), "answer 2".into()),
        ]
    );
}

#[tokio::test]
async fn native_steer_checkpoint_precedes_delivery_and_streaming_continuation() {
    use oc_core::queries::{InboxDelivery, NoticePlacement, TranscriptPart};
    use tokio::io::AsyncWriteExt;

    let f = RunningInputFixture::new(|_| {}).await;
    let (mut first, _) = f.request().await;
    let mut events = f.owner.resources.live.subscribe();
    f.run
        .app
        .enqueue(SessionId("R".into()), "steer".into(), InboxDelivery::Steer)
        .await
        .unwrap();
    respond(&mut first, json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"before steer"}]}])).await;
    // Holding the next response prevents the terminal projection from repairing
    // missing native checkpoints and hiding the active-turn ordering defect.
    let (mut second, body) = f.request().await;
    assert!(body["input"].to_string().contains("steer"));
    let mut checkpoint = None;
    let mut delivered = false;
    while let Ok(publication) = events.try_recv() {
        match publication.event {
            CoreEvent::TurnPresentation {
                turn, projection, ..
            } if turn == f.turn => {
                checkpoint = Some(projection);
            }
            CoreEvent::InboxChanged {
                delivered: items, ..
            } if !items.is_empty() => {
                let projection = checkpoint
                    .as_ref()
                    .expect("native checkpoint before delivery");
                assert_eq!(projection.status, "started");
                assert_eq!(
                    projection.parts,
                    [TranscriptPart::Text("before steer".into())]
                );
                let span = projection.spans.last().unwrap();
                assert_eq!(span.step, 2);
                let notice = &projection.notice_order.as_ref().unwrap().notices[0];
                assert_eq!(notice.message, items[0].message);
                assert_eq!(
                    notice.placement,
                    NoticePlacement::BeforeRequest {
                        span: span.id.clone(),
                        before_part_sequence: projection.window.as_ref().unwrap().end,
                    }
                );
                delivered = true;
            }
            _ => {}
        }
    }
    assert!(delivered, "steer was delivered without waiting for idle");

    let delta = format!(
        "data: {}\n\n",
        json!({"type":"response.output_text.delta","delta":"after steer"})
    );
    let completed = format!(
        "data: {}\n\n",
        json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"after steer"}]}]}})
    );
    second.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{delta}", delta.len() + completed.len()).as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut prepared = false;
        loop {
            match events.recv().await.unwrap().event {
                CoreEvent::TurnPresentation { turn, .. } if turn == f.turn => prepared = true,
                CoreEvent::TextDelta { turn, delta, .. } if turn == f.turn => {
                    assert!(
                        prepared,
                        "semantic-start checkpoint must reach native before its delta"
                    );
                    assert_eq!(delta, "after steer");
                    break;
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    second.write_all(completed.as_bytes()).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn user_interrupt_resumes_only_steers_until_a_new_input_is_admitted() {
    use oc_core::queries::InboxDelivery;
    use tokio::io::AsyncReadExt;

    // A fresh admission during the successor deliberately restores normal
    // draining; without it even a successful successor must leave Queue parked.
    for new_admission in [false, true] {
        let fixture = RunningInputFixture::new(|_| {}).await;
        let session = SessionId("R".into());
        let (mut original, _) = fixture.request().await;
        fixture
            .run
            .app
            .register_approval_consumer(true)
            .await
            .unwrap();
        let queued = fixture
            .run
            .app
            .enqueue(session.clone(), "queued".into(), InboxDelivery::Queue)
            .await
            .unwrap();
        fixture
            .run
            .app
            .enqueue(session.clone(), "steer".into(), InboxDelivery::Steer)
            .await
            .unwrap();
        fixture
            .owner
            .cancel_root(
                &fixture.run.location,
                "R",
                fixture.run.origin.id,
                Some(&fixture.turn),
                true,
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), fixture.owner.next_root())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), original.read(&mut [0]))
                .await
                .unwrap()
                .unwrap(),
            0
        );

        let (mut socket, body) = fixture.request().await;
        let resumed = fixture.owner.root("R").unwrap();
        assert!(Arc::ptr_eq(&resumed.origin, &fixture.run.origin));
        assert!(!Arc::ptr_eq(&resumed.cancel, &fixture.run.cancel));
        assert!(fixture.run.cancel.load(Ordering::Acquire));
        assert!(!resumed.cancel.load(Ordering::Acquire));
        assert!(!resumed.origin.flag.load(Ordering::Acquire));
        assert!(resumed.origin.auto_once.load(Ordering::Acquire));
        assert_eq!(
            fixture.owner.cancel_root(
                &resumed.location,
                "R",
                resumed.origin.id,
                Some(&fixture.turn),
                true
            ),
            Err(CoreError::TurnNotActive)
        );
        let input = body["input"].to_string();
        assert!(input.contains("steer") && !input.contains("queued"));
        if new_admission {
            resumed
                .app
                .enqueue(session.clone(), "new queued".into(), InboxDelivery::Queue)
                .await
                .unwrap();
        }
        final_answer(&mut socket).await;
        if new_admission {
            for text in ["queued", "new queued"] {
                let (mut socket, body) = fixture.request().await;
                assert!(
                    body["input"]
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap()
                        .to_string()
                        .contains(text)
                );
                final_answer(&mut socket).await;
            }
        }
        fixture.finish().await;
        assert!(fixture.owner.root("R").is_none());
        assert!(futures_util::FutureExt::now_or_never(fixture.listener.accept()).is_none());
        let pending = fixture.owner.db.inbox_pending("R").unwrap();
        assert_eq!(
            pending,
            if new_admission {
                Vec::new()
            } else {
                vec![queued]
            }
        );
        let history = fixture.owner.db.read_history("R").unwrap();
        for text in ["original", "steer"] {
            assert_eq!(
                history
                    .iter()
                    .filter(|(role, value)| role == "user" && value == text)
                    .count(),
                1
            );
        }
    }
}

#[tokio::test]
async fn origin_or_owner_shutdown_suppresses_an_interrupted_steer_successor() {
    use oc_core::queries::InboxDelivery;

    for shutdown in [false, true] {
        let fixture = RunningInputFixture::new(|_| {}).await;
        let (_socket, _) = fixture.request().await;
        let pending = fixture
            .run
            .app
            .enqueue(SessionId("R".into()), "steer".into(), InboxDelivery::Steer)
            .await
            .unwrap();
        fixture
            .owner
            .cancel_root(
                &fixture.run.location,
                "R",
                fixture.run.origin.id,
                Some(&fixture.turn),
                true,
            )
            .unwrap();
        if shutdown {
            fixture.owner.shutdown().await.unwrap();
        } else {
            fixture.owner.cancel_origin(&fixture.run.origin);
            fixture.finish().await;
        }
        assert!(fixture.owner.root("R").is_none());
        assert_eq!(fixture.owner.db.inbox_pending("R").unwrap(), [pending]);
        assert!(futures_util::FutureExt::now_or_never(fixture.listener.accept()).is_none());
        assert_eq!(
            fixture.owner.db.read_history("R").unwrap(),
            [("user".into(), "original".into())]
        );
    }
}

struct RunningInputFixture {
    _root: tempfile::TempDir,
    project: PathBuf,
    owner: Arc<Owner>,
    run: Arc<Root>,
    listener: tokio::net::TcpListener,
    turn: oc_core::core_app::WorkerTurnId,
}

impl RunningInputFixture {
    async fn new(configure: impl FnOnce(&mut serde_json::Value)) -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
        configure(&mut source);
        std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
        let owner = Owner::new(
            Db::open(&root.path().join("data")).unwrap(),
            env(root.path()),
        )
        .unwrap();
        let location = owner.location(&project, false).await.unwrap();
        location
            .app
            .create_session(SessionId("R".into()))
            .await
            .unwrap();
        location
            .app
            .rename_session(SessionId("R".into()), "fixed".into())
            .await
            .unwrap();
        let run = owner
            .begin(
                location,
                "R".into(),
                Arc::new(Origin::new(Kind::Run, false)),
                1,
                false,
                None,
            )
            .unwrap();
        let turn = run
            .app
            .submit(SessionId("R".into()), "original".into())
            .await
            .unwrap();
        Self {
            _root: root,
            project,
            owner,
            run,
            listener,
            turn,
        }
    }

    async fn request(&self) -> (tokio::net::TcpStream, serde_json::Value) {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (mut socket, _) = self.listener.accept().await.unwrap();
            let body = request(&mut socket).await;
            (socket, body)
        })
        .await
        .unwrap()
    }

    async fn finish(&self) {
        tokio::time::timeout(Duration::from_secs(5), self.owner.next_root())
            .await
            .unwrap()
            .unwrap();
        self.owner.shutdown().await.unwrap();
    }
}

fn image_prompt(label: &str) -> oc_core::prompt::PromptInput {
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let prefix = format!("{label} Ω ");
    oc_core::prompt::PromptInput {
        text: format!("{prefix}[Image 1]"),
        images: vec![oc_core::prompt::PromptImage {
            mark: oc_core::prompt::ImageMark {
                start: prefix.len(),
                end: prefix.len() + 9,
                ordinal: 1,
            },
            png: png.into_inner().into(),
        }],
    }
}

async fn final_answer(socket: &mut tokio::net::TcpStream) {
    respond(socket,json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}])).await;
}

#[tokio::test]
async fn running_rich_images_steer_and_queue_reach_real_wire_and_keep_distinct_message_metadata() {
    use oc_core::queries::InboxDelivery;
    let f = RunningInputFixture::new(|c| {
        c["providers"]["fixture"]["models"]["m"]["capabilities"] = json!({"input":["text","image"]})
    })
    .await;
    let (mut first, _) = f.request().await;
    let steer = image_prompt("steered");
    let queued = image_prompt("queued");
    f.run
        .app
        .enqueue(SessionId("R".into()), queued.clone(), InboxDelivery::Queue)
        .await
        .unwrap();
    let item = f
        .run
        .app
        .enqueue(SessionId("R".into()), steer.clone(), InboxDelivery::Steer)
        .await
        .unwrap();
    assert_eq!(item.images, 1);
    final_answer(&mut first).await;
    let (mut second, body) = f.request().await;
    assert!(body["input"].to_string().contains("steered"));
    assert!(!body["input"].to_string().contains("queued"));
    let images = |body: &serde_json::Value| {
        body["input"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|i| i["content"].as_array().into_iter().flatten())
            .filter(|c| c["type"] == "input_image")
            .count()
    };
    assert_eq!(images(&body), 1);
    final_answer(&mut second).await;
    let (mut third, body) = f.request().await;
    assert_eq!(images(&body), 2);
    assert!(body["input"].to_string().contains("queued"));
    final_answer(&mut third).await;
    f.finish().await;
    let history = f.owner.db.read_history("R").unwrap();
    assert_eq!(history.iter().filter(|(role, _)| role == "user").count(), 3);
    let log: serde_json::Value =
        serde_json::from_str(&f.owner.db.turn_result(&f.turn.0).unwrap().1.unwrap()).unwrap();
    assert_eq!(log["requests"].as_array().unwrap().len(), 2);
    assert!(f.owner.db.inbox_pending("R").unwrap().is_empty());
}

#[tokio::test]
async fn running_rich_steer_retry_keeps_one_delivery_and_does_not_adopt_a_later_input() {
    use oc_core::queries::InboxDelivery;
    use tokio::io::AsyncWriteExt;
    let f = RunningInputFixture::new(|c| {
        c["providers"]["fixture"]["models"]["m"]["capabilities"] =
            json!({"input":["text","image"]});
    })
    .await;
    let (mut first, _) = f.request().await;
    let prompt = image_prompt("retry image");
    f.run
        .app
        .enqueue(SessionId("R".into()), prompt.clone(), InboxDelivery::Steer)
        .await
        .unwrap();
    final_answer(&mut first).await;
    let (mut second, issued) = f.request().await;
    second.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    drop(second);
    f.run
        .app
        .enqueue(
            SessionId("R".into()),
            "LATER_INPUT".into(),
            InboxDelivery::Steer,
        )
        .await
        .unwrap();
    let (mut retry, retried) = f.request().await;
    assert_eq!(issued["input"], retried["input"]);
    assert!(!retried["input"].to_string().contains("LATER_INPUT"));
    final_answer(&mut retry).await;
    let (mut next, body) = f.request().await;
    assert!(body["input"].to_string().contains("LATER_INPUT"));
    final_answer(&mut next).await;
    f.finish().await;
    let history = f.owner.db.read_history("R").unwrap();
    assert_eq!(history.iter().filter(|(role, _)| role == "user").count(), 3);
    assert_eq!(
        history
            .iter()
            .filter(|(_, text)| text == &prompt.text)
            .count(),
        1
    );
    assert!(f.owner.db.inbox_pending("R").unwrap().is_empty());
}

#[tokio::test]
async fn running_inline_commands_preserve_expansion_original_and_agent_model_routing_for_both_deliveries()
 {
    use oc_core::queries::InboxDelivery;
    for delivery in [InboxDelivery::Steer, InboxDelivery::Queue] {
        let f = RunningInputFixture::new(|c| {
            c["agents"]["reviewer"]=json!({"mode":"subagent","system":"INLINE_REVIEWER"});
            c["providers"]["fixture"]["models"]["override"]=json!({"limit":{"context":100000,"output":2048}});
            c["commands"]=json!({"review":{"template":"EXPANDED $1","agent":"reviewer","subagent":false,"model":"fixture/override"}});
        }).await;
        let (mut first, _) = f.request().await;
        f.run
            .app
            .enqueue(SessionId("R".into()), "/review ARG".into(), delivery)
            .await
            .unwrap();
        final_answer(&mut first).await;
        let (mut next, body) = f.request().await;
        assert_eq!(body["model"], "override");
        let input = body["input"].to_string();
        assert!(input.contains("INLINE_REVIEWER") && input.contains("EXPANDED ARG"));
        assert!(!input.contains("EXPANDED EXPANDED"));
        final_answer(&mut next).await;
        f.finish().await;
        assert!(
            f.owner
                .db
                .read_history("R")
                .unwrap()
                .contains(&("user".into(), "/review ARG".into()))
        );
        let log: serde_json::Value =
            serde_json::from_str(&f.owner.db.turn_result(&f.turn.0).unwrap().1.unwrap()).unwrap();
        assert_eq!(
            log["requests"].as_array().unwrap().len(),
            if delivery == InboxDelivery::Steer {
                2
            } else {
                1
            }
        );
    }
}

#[tokio::test]
async fn running_background_commands_have_unique_real_child_operations_without_finishing_parent_early()
 {
    use oc_core::queries::InboxDelivery;
    for delivery in [InboxDelivery::Steer, InboxDelivery::Queue] {
        let f = RunningInputFixture::new(|c| {
            c["agents"]["helper"] = json!({"mode":"subagent","model":"fixture/child"});
            c["providers"]["fixture"]["models"]["child"] =
                json!({"limit":{"context":100000,"output":2048}});
            c["commands"] =
                json!({"bg":{"template":"CHILD_EXPANDED $1","agent":"helper","subagent":true}});
            c["permissions"] = json!([{"action":"subagent","resource":"*","effect":"allow"}]);
        })
        .await;
        let (mut first, _) = f.request().await;
        for invocation in ["/bg one", "/bg two"] {
            f.run
                .app
                .enqueue(SessionId("R".into()), invocation.into(), delivery)
                .await
                .unwrap();
        }
        final_answer(&mut first).await;
        let mut children = Vec::new();
        let mut parent = None;
        for _ in 0..if delivery == InboxDelivery::Steer {
            3
        } else {
            2
        } {
            let (socket, body) = f.request().await;
            if body["model"] == "child" {
                assert!(body["input"].to_string().contains("CHILD_EXPANDED"));
                children.push(socket);
            } else {
                assert!(body["input"].to_string().contains("function_call_output"));
                parent = Some(socket);
            }
        }
        assert_eq!(children.len(), 2);
        if let Some(mut parent) = parent {
            final_answer(&mut parent).await;
        }
        tokio::time::timeout(Duration::from_secs(5), f.owner.next_root())
            .await
            .unwrap()
            .unwrap();
        let jobs = f.owner.db.child_jobs("R").unwrap();
        assert_eq!(jobs.len(), 2);
        assert_ne!(jobs[0].operation, jobs[1].operation);
        if delivery == InboxDelivery::Steer {
            let log: serde_json::Value =
                serde_json::from_str(&f.owner.db.turn_result(&f.turn.0).unwrap().1.unwrap())
                    .unwrap();
            assert_eq!(log["display"]["commands"].as_object().unwrap().len(), 2);
        }
        f.owner.cancel_origin(&f.run.origin);
        drop(children);
        f.owner.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn external_directory_replies_route_from_location_to_root_and_child() {
    use oc_core::approval::{ApprovalDecision, ApprovalPreview, ApprovalReply};
    for (child, decision) in [
        (false, ApprovalDecision::Once),
        (false, ApprovalDecision::Reject { feedback: None }),
        (true, ApprovalDecision::Once),
        (
            true,
            ApprovalDecision::Reject {
                feedback: Some("use webfetch instead".into()),
            },
        ),
    ] {
        let f = RunningInputFixture::new(|c| {
            c["agents"]["helper"] = json!({"mode":"subagent"});
            c["permissions"] = json!([
                {"action":"subagent","resource":"*","effect":"allow"},
                {"action":"read","resource":"*","effect":"allow"},
                {"action":"external_directory","resource":"*","effect":"ask"}
            ]);
        })
        .await;
        let external = f._root.path().join("external.txt");
        std::fs::write(&external, "EXTERNAL_APPROVED_CONTENT").unwrap();
        let mut events = f.run.app.subscribe();
        let (mut socket, _) = f.request().await;
        if child {
            respond(&mut socket, json!([{"type":"function_call","call_id":"delegate","name":"subagent","arguments":json!({"agent":"helper","description":"Read external source","prompt":"Read external source"}).to_string()}])).await;
            (socket, _) = f.request().await;
        }
        respond(&mut socket, json!([{"type":"function_call","call_id":"external-read","name":"read","arguments":json!({"path":external}).to_string()}])).await;
        let approval = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match events.recv().await.unwrap() {
                    CoreEvent::PermissionAsked(request) => break request,
                    CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(approval.action, "external_directory");
        assert_eq!(
            approval.preview,
            ApprovalPreview::Resource {
                values: approval.resources.clone(),
            }
        );
        assert_eq!(approval.binding.session != "R", child);
        assert!(approval.save_patterns.is_empty());
        assert!(
            f.owner
                .db
                .list_tool_ops(&approval.binding.session)
                .unwrap()
                .is_empty()
        );
        // Native replies go through the Location, not directly to the primary.
        let app = &f.run.location.app;
        assert_eq!(
            app.pending_approvals().await.unwrap().as_slice(),
            std::slice::from_ref(&approval)
        );
        let mut reply = ApprovalReply {
            id: approval.id,
            binding: approval.binding.clone(),
            decision: decision.clone(),
        };
        reply.binding.input_digest.push_str("wrong");
        assert!(app.reply_approval(reply.clone()).await.is_err());
        assert_eq!(
            app.pending_approvals().await.unwrap().as_slice(),
            std::slice::from_ref(&approval)
        );
        reply.binding = approval.binding;
        app.reply_approval(reply.clone()).await.unwrap();
        assert!(app.pending_approvals().await.unwrap().is_empty());
        assert!(app.reply_approval(reply.clone()).await.is_err());
        if decision != (ApprovalDecision::Reject { feedback: None }) {
            let (mut next, body) = f.request().await;
            let output = body["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == "external-read"
                })
                .unwrap()["output"]
                .as_str()
                .unwrap();
            match &decision {
                ApprovalDecision::Once => assert!(output.contains("EXTERNAL_APPROVED_CONTENT")),
                ApprovalDecision::Reject {
                    feedback: Some(feedback),
                } => {
                    assert!(output.contains(feedback));
                    assert!(!output.contains("EXTERNAL_APPROVED_CONTENT"));
                }
                _ => unreachable!(),
            }
            final_answer(&mut next).await;
            if child {
                let (mut parent, _) = f.request().await;
                final_answer(&mut parent).await;
            }
        }
        f.finish().await;
        let ops = f.owner.db.list_tool_ops(&reply.binding.session).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0].state,
            if decision == ApprovalDecision::Once {
                "completed"
            } else {
                "denied"
            }
        );
        let conn = rusqlite::Connection::open_with_flags(
            f.owner.db.root().join("oc.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM permission_grants", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn running_shell_ask_remains_pumped_after_model_finishes_and_receipt_cancel_does_not_cancel_model()
 {
    use oc_core::{
        approval::{ApprovalDecision, ApprovalReply},
        core_app::UserShellSelection,
    };
    let f = RunningInputFixture::new(|c| {
        c["permissions"] = json!([{"action":"shell","resource":"*","effect":"ask"}])
    })
    .await;
    let (mut first, _) = f.request().await;
    let mut events = f.run.app.subscribe();
    let mut cancelled = f
        .run
        .app
        .request_user_shell(
            SessionId("R".into()),
            "printf no > cancelled.effect".into(),
            UserShellSelection::Existing(None),
        )
        .unwrap();
    let approval = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::PermissionAsked(request) = events.recv().await.unwrap() {
                break request;
            }
        }
    })
    .await
    .unwrap();
    assert!(approval.binding.operation.starts_with("user-shell:"));
    cancelled.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), cancelled.wait())
            .await
            .unwrap()
            .is_err()
    );
    assert!(!f.run.cancel.load(Ordering::Acquire));
    assert!(!f.project.join("cancelled.effect").exists());
    let mut shell = f
        .run
        .app
        .request_user_shell(
            SessionId("R".into()),
            "printf yes > allowed.effect".into(),
            UserShellSelection::Existing(None),
        )
        .unwrap();
    let approval = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::PermissionAsked(request) = events.recv().await.unwrap() {
                break request;
            }
        }
    })
    .await
    .unwrap();
    final_answer(&mut first).await;
    // Model completion cannot retire its actor while the independent Ask waits.
    assert!(!f.run.app.pending_approvals().await.unwrap().is_empty());
    f.run
        .app
        .reply_approval(ApprovalReply {
            id: approval.id,
            binding: approval.binding,
            decision: ApprovalDecision::Once,
        })
        .await
        .unwrap();
    let operation = tokio::time::timeout(Duration::from_secs(5), shell.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(operation.starts_with("user-shell:"));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !f.project.join("allowed.effect").exists() {
            events.recv().await.unwrap();
        }
    })
    .await
    .unwrap();
    f.finish().await;
    assert_eq!(
        std::fs::read_to_string(f.project.join("allowed.effect")).unwrap(),
        "yes"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(20), f.listener.accept())
            .await
            .is_err(),
        "no provider request merely to execute shell"
    );
}

#[tokio::test]
async fn running_stale_command_is_visible_restorable_and_does_not_reexpand_or_request_again() {
    use oc_core::queries::InboxDelivery;
    for interrupted in [false, true] {
        let f = RunningInputFixture::new(|c| {
            c["commands"] = json!({"check":{"template":"ORIGINAL_EXPANSION $1"}})
        })
        .await;
        let (mut first, _) = f.request().await;
        let item = f
            .run
            .app
            .enqueue(
                SessionId("R".into()),
                "/check ARG".into(),
                InboxDelivery::Steer,
            )
            .await
            .unwrap();
        if interrupted {
            f.run
                .app
                .enqueue(
                    SessionId("R".into()),
                    "valid steer".into(),
                    InboxDelivery::Steer,
                )
                .await
                .unwrap();
        }
        let path = f.project.join("opencode.json");
        let mut source: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        source["commands"]["check"]["template"] = json!("CHANGED_EXPANSION $1");
        std::fs::write(path, source.to_string()).unwrap();
        // Reload belongs to the Location owner, not its running source follower.
        let location = f.owner.location(&f.project, false).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), location.app.reload_location())
            .await
            .unwrap()
            .unwrap();
        if interrupted {
            f.owner
                .cancel_root(&location, "R", f.run.origin.id, Some(&f.turn), true)
                .unwrap();
            tokio::time::timeout(Duration::from_secs(5), f.owner.next_root())
                .await
                .unwrap()
                .unwrap();
            let (mut next, body) = f.request().await;
            assert!(
                body["input"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap()
                    .to_string()
                    .contains("valid steer")
            );
            final_answer(&mut next).await;
        } else {
            final_answer(&mut first).await;
        }
        f.finish().await;
        let pending = f.owner.db.inbox_read("R", &item.id).unwrap().unwrap();
        assert!(pending.item.error.is_some());
        assert_eq!(pending.payload.original().text, "/check ARG");
        assert_eq!(pending.payload.prompt.text, "ORIGINAL_EXPANSION ARG");
        assert!(
            tokio::time::timeout(Duration::from_millis(20), f.listener.accept())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn running_command_reopen_rejects_changed_sources_even_when_generation_numbers_repeat() {
    use oc_core::queries::InboxDelivery;
    for changed in 0..3 {
        let f = RunningInputFixture::new(|c| {
            c["agents"]["alpha"] = json!({"system":"REOPEN_PARENT"});
            c["agents"]["reviewer"] = json!({"mode":"primary","system":"REOPEN_TARGET"});
            c["default_agent"] = json!("alpha");
            c["commands"] = json!({"reopen":{"template":"REOPEN_EXPANSION $1","agent":"reviewer"}});
        })
        .await;
        let (first, _) = f.request().await;
        let item = f
            .run
            .app
            .enqueue(
                SessionId("R".into()),
                "/reopen ARG".into(),
                InboxDelivery::Queue,
            )
            .await
            .unwrap();
        let generation = f
            .owner
            .db
            .inbox_read("R", &item.id)
            .unwrap()
            .unwrap()
            .payload
            .command
            .unwrap()
            .generation;
        f.run.app.cancel(SessionId("R".into())).await.unwrap();
        drop(first);
        f.finish().await;
        let RunningInputFixture {
            _root,
            project,
            owner,
            run,
            listener,
            ..
        } = f;
        drop(run);
        drop(owner);

        let path = project.join("opencode.json");
        let mut source: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        match changed {
            1 => source["commands"]["reopen"]["template"] = json!("DIFFERENT_EXPANSION $1"),
            2 => source["agents"]["reviewer"]["mode"] = json!("subagent"),
            _ => {}
        }
        std::fs::write(path, source.to_string()).unwrap();
        let owner = Owner::new(
            Db::open(&_root.path().join("data")).unwrap(),
            env(_root.path()),
        )
        .unwrap();
        let location = owner.location(&project, false).await.unwrap();
        assert_eq!(
            location
                .app
                .catalog()
                .await
                .unwrap()
                .chrome
                .selection_generation,
            generation
        );
        let run = owner
            .begin(
                location,
                "R".into(),
                Arc::new(Origin::new(Kind::Run, false)),
                1,
                false,
                None,
            )
            .unwrap();
        let (ack, receipt) = oneshot::channel();
        run.app
            .dispatch(InboxMsg::SubmitInbox {
                session: SessionId("R".into()),
                id: item.id.clone(),
                ack,
            })
            .await
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), receipt)
            .await
            .unwrap()
            .unwrap();
        if changed == 0 {
            result.unwrap();
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let body = request(&mut socket).await;
            let input = body["input"].to_string();
            assert!(input.contains("REOPEN_TARGET") && input.contains("REOPEN_EXPANSION ARG"));
            final_answer(&mut socket).await;
        } else {
            assert!(result.is_err());
            let pending = owner.db.inbox_read("R", &item.id).unwrap().unwrap();
            assert!(pending.item.error.is_some());
            assert_eq!(pending.payload.original().text, "/reopen ARG");
            assert_eq!(pending.payload.prompt.text, "REOPEN_EXPANSION ARG");
            assert!(
                tokio::time::timeout(Duration::from_millis(20), listener.accept())
                    .await
                    .is_err()
            );
        }
        tokio::time::timeout(Duration::from_secs(5), owner.next_root())
            .await
            .unwrap()
            .unwrap();
        owner.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn running_shell_one_slot_and_root_cancel_preserve_pending_prompts_without_launch() {
    use oc_core::{core_app::UserShellSelection, queries::InboxDelivery};
    let f = RunningInputFixture::new(|c| {
        c["permissions"] = json!([{"action":"shell","resource":"*","effect":"ask"}])
    })
    .await;
    let (_first, _) = f.request().await;
    let mut events = f.run.app.subscribe();
    let mut shell = f
        .run
        .app
        .request_user_shell(
            SessionId("R".into()),
            "printf no > cancelled-root.effect".into(),
            UserShellSelection::Existing(None),
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::PermissionAsked(_)) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let mut second = f
        .run
        .app
        .request_user_shell(
            SessionId("R".into()),
            "printf duplicate".into(),
            UserShellSelection::Existing(None),
        )
        .unwrap();
    assert_eq!(second.wait().await, Err(CoreError::TurnBusy));
    f.run
        .app
        .enqueue(
            SessionId("R".into()),
            "retained after cancel".into(),
            InboxDelivery::Queue,
        )
        .await
        .unwrap();
    f.run.app.cancel(SessionId("R".into())).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), shell.wait())
            .await
            .unwrap()
            .is_err()
    );
    f.finish().await;
    assert!(!f.project.join("cancelled-root.effect").exists());
    assert_eq!(f.owner.db.inbox_pending("R").unwrap().len(), 1);
}

#[tokio::test]
async fn running_shell_capture_does_not_overwrite_a_later_committed_agent() {
    use oc_core::{
        approval::{ApprovalDecision, ApprovalReply},
        core_app::UserShellSelection,
    };
    let f = RunningInputFixture::new(|c| {
        c["agents"]["alpha"] = json!({"system":"ALPHA"});
        c["agents"]["beta"] = json!({"system":"BETA"});
        c["default_agent"] = json!("alpha");
        c["permissions"] = json!([{"action":"shell","resource":"*","effect":"ask"}]);
    })
    .await;
    let (mut first, _) = f.request().await;
    let mut events = f.run.app.subscribe();
    let mut shell = f
        .run
        .app
        .request_user_shell(
            SessionId("R".into()),
            "printf captured > captured.effect".into(),
            UserShellSelection::Existing(None),
        )
        .unwrap();
    let approval = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::PermissionAsked(request) = events.recv().await.unwrap() {
                break request;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(approval.binding.agent.as_deref(), Some("alpha"));
    f.run
        .app
        .session_selection(
            SessionId("R".into()),
            false,
            SessionSelectionAction::Agent("beta".into()),
        )
        .await
        .unwrap();
    f.run
        .app
        .reply_approval(ApprovalReply {
            id: approval.id,
            binding: approval.binding,
            decision: ApprovalDecision::Once,
        })
        .await
        .unwrap();
    shell.wait().await.unwrap();
    assert_eq!(
        f.run
            .app
            .session_selection(
                SessionId("R".into()),
                false,
                SessionSelectionAction::Current
            )
            .await
            .unwrap()
            .agent_id
            .as_deref(),
        Some("beta")
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !f.project.join("captured.effect").exists() {
            events.recv().await.unwrap();
        }
    })
    .await
    .unwrap();
    final_answer(&mut first).await;
    f.finish().await;
}
