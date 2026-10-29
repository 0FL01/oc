//! A longer session must not make its newest history page, the newest turn's
//! presentation, per-row classification or session listing more expensive.
use super::vm_steps::{assert_flat, install, steps};
use super::*;
use oc_core::{domain::SessionId, session::MessageId};
use serde_json::json;

struct Session {
    _dir: tempfile::TempDir,
    db: Db,
    last_turn: String,
    last_user: MessageId,
}

/// Real accept/tool/checkpoint/commit paths: `turns` turns of three tools.
fn session(turns: usize) -> Session {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    // Population speed only: the measured statements do not depend on fsync.
    db.conn
        .lock()
        .unwrap()
        .pragma_update(None, "synchronous", "OFF")
        .unwrap();
    db.create_session("s").unwrap();
    let model = oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    let mut last_user = String::new();
    for t in 0..turns {
        let turn = format!("turn-{t}");
        last_user = db
            .accept_turn(&turn, "s", "question", "question", &model)
            .unwrap()
            .user_message;
        let mut parts = Vec::new();
        for k in 0..3 {
            let op = format!("op-{t}-{k}");
            let input = json!({"path": format!("src/f{k}.rs")}).to_string();
            db.record_tool_intent(&op, "s", Some(&turn), "read", &input)
                .unwrap();
            db.record_tool_outcome(&op, "completed", Some("content"))
                .unwrap();
            parts.push(json!({"tool": op}));
        }
        parts.push(json!({"message": 0}));
        let log = json!({
            "display_parts": parts,
            "input": [{"role":"assistant","content":[{"type":"output_text","text":format!("answer {t}")}]}],
        })
        .to_string();
        db.checkpoint_turn(&turn, &log).unwrap();
        db.commit_turn(&turn, "completed", Some(&log), Some(&format!("answer {t}")))
            .unwrap();
    }
    install(&db.conn.lock().unwrap());
    Session {
        _dir: dir,
        db,
        last_turn: format!("turn-{}", turns - 1),
        last_user: MessageId(last_user),
    }
}

fn measure(session: &Session) -> Vec<(&'static str, i64)> {
    let db = &session.db;
    let user = &session.last_user;
    let (page, page_steps) = steps(|| db.read_history_page_typed("s", 100, None).unwrap());
    assert_eq!(page.len(), 100);
    let (view, view_steps) = steps(|| db.turn_presentation("s", &session.last_turn).unwrap());
    assert_eq!(view.expect("newest turn").parts.len(), 4);
    let (child, child_steps) = steps(|| db.child_history("s", user).unwrap());
    assert!(child.is_none());
    let (shell, shell_steps) = steps(|| db.user_shell_result("s", user).unwrap());
    assert!(shell.is_none());
    let (notice, notice_steps) = steps(|| db.model_shell_notice("s", user).unwrap());
    assert!(notice.is_none());
    let (images, images_steps) = steps(|| db.user_images("s", &user.0, "question").unwrap());
    assert!(images.is_empty());
    let (list, list_steps) = steps(|| db.session_list("", None).unwrap());
    assert_eq!(list.len(), 1);
    let (activity, activity_steps) =
        steps(|| db.session_activity(&[SessionId("s".into())]).unwrap());
    assert_eq!(activity.len(), 1);
    vec![
        ("read_history_page_typed", page_steps),
        ("turn_presentation", view_steps),
        ("child_history", child_steps),
        ("user_shell_result", shell_steps),
        ("model_shell_notice", notice_steps),
        ("user_images", images_steps),
        ("session_list", list_steps),
        ("session_activity", activity_steps),
    ]
}

#[test]
fn newest_history_reads_do_not_grow_with_session_length() {
    let (small, large) = (50, 200);
    let before = measure(&session(small));
    let after = measure(&session(large));
    assert_flat((small, large), &before, &after);
}

/// The indexed busy probe walks up from running work; it must still mean
/// "the session or one of its descendants runs a turn or an active child job".
#[test]
fn busy_family_follows_running_turns_and_active_child_jobs_upward() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("root").unwrap();
    db.create_session("other").unwrap();
    db.create_child_session("root", "child", None, None, None)
        .unwrap();
    db.create_child_session("child", "grandchild", None, None, None)
        .unwrap();
    let busy = |db: &Db| -> Vec<bool> {
        db.session_activity(
            &["root", "child", "grandchild", "other"].map(|id| SessionId(id.into())),
        )
        .unwrap()
        .into_iter()
        .map(|activity| activity.busy)
        .collect()
    };
    let listed = |db: &Db| -> Vec<(String, bool)> {
        let mut rows: Vec<_> = db
            .session_list("", None)
            .unwrap()
            .into_iter()
            .map(|entry| (entry.id.0, entry.running))
            .collect();
        rows.sort();
        rows
    };
    let idle_list = vec![("other".to_string(), false), ("root".to_string(), false)];
    assert_eq!(busy(&db), [false, false, false, false]);
    assert_eq!(listed(&db), idle_list);

    db.begin_turn("deep", "grandchild", "work").unwrap();
    assert_eq!(busy(&db), [true, true, true, false]);
    assert_eq!(
        listed(&db),
        [("other".to_string(), false), ("root".to_string(), true)]
    );
    db.finish_turn("deep", "completed", None).unwrap();
    assert_eq!(busy(&db), [false, false, false, false]);
    assert_eq!(listed(&db), idle_list);

    db.create_child_session("child", "worker", None, None, None)
        .unwrap();
    db.record_tool_intent("launch", "child", None, "subagent", "{}")
        .unwrap();
    let job = oc_core::queries::ChildJob {
        parent: SessionId("child".into()),
        child: SessionId("worker".into()),
        operation: "launch".into(),
        generation: 1,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/child".into(),
        description: "work".into(),
        delivery_id: "child-delivery:launch".into(),
        state: oc_core::queries::ChildState::Admitted,
        background: true,
        turn: None,
        result: None,
        message_id: None,
    };
    db.admit_child_job(&job).unwrap();
    assert_eq!(
        busy(&db),
        [true, true, false, false],
        "active job marks its parent family"
    );
    db.finish_child_job("launch", oc_core::queries::ChildState::Completed, "done")
        .unwrap();
    assert_eq!(busy(&db), [false, false, false, false]);
}
