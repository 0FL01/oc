//! TOOL18 real runtime admission, storage transaction and frontend publication.
use super::*;
use serde_json::json;

#[tokio::test]
async fn tool18_rename_real_owner_publication_rollback_and_exact_resource() {
    let (h, mut generation) = make_harness(allow_all());
    generation
        .permissions
        .insert("opencode_session_rename".into(), Permission::Allow);
    generation.permission_rules=oc_adapters::permissions::PermissionRules::from_config(&json!({"permission":{"opencode_session_rename":{"*":"deny","current":"allow","other":"allow","rollback":"allow"}}})).unwrap();
    let runtime = runtime_of(&h, generation, vec![]);
    for s in ["current", "other", "blocked", "rollback"] {
        runtime.create_session(s).unwrap();
    }
    let connection = rusqlite::Connection::open(h._data.path().join("oc.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_rename_event BEFORE INSERT ON events WHEN NEW.kind='session_updated' AND NEW.session_id='rollback' BEGIN SELECT RAISE(ABORT,'private'); END;").unwrap();
    let (events, mut rx) = tokio::sync::broadcast::channel(32);
    runtime.set_approval_events(&events);
    let calls = [
        ("self", json!({"title":"  Current 🦀  "})),
        ("other", json!({"title":"Other","sessionID":"other"})),
        ("deny", json!({"title":"No","sessionID":"blocked"})),
        ("missing", json!({"title":"No","sessionID":"ghost"})),
        (
            "rollback",
            json!({"title":"Rollback","sessionID":"rollback"}),
        ),
    ];
    let body = calls
        .iter()
        .map(|(id, args)| sse_tool_call(id, "opencode_session_rename", args))
        .collect::<String>()
        + &sse_completed();
    let (base, _, requests) = Fake::start_recording(vec![body, sse_completed()], Duration::ZERO);
    let mut starts = Vec::new();
    let result = runtime
        .run_turn_with_tool_events(
            params("current", "rename", &h, provider_of(&base), &NO_CANCEL),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if let ToolCallEvent::Started { op, .. } = event {
                    starts.push(op.clone());
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(
        result
            .calls
            .iter()
            .map(|r| r.state.as_str())
            .collect::<Vec<_>>(),
        ["completed", "completed", "failed", "failed", "failed"]
    );
    let title = |s| {
        connection
            .query_row("SELECT title FROM sessions WHERE id=?1", [s], |r| {
                r.get::<_, Option<String>>(0)
            })
            .unwrap()
    };
    assert_eq!(title("current").as_deref(), Some("Current 🦀"));
    assert_eq!(title("other").as_deref(), Some("Other"));
    assert!(title("blocked").is_none() && title("rollback").is_none());
    let mut published = vec![];
    while let Ok(event) = rx.try_recv() {
        if let oc_core::core_app::CoreEvent::SessionTitleUpdated { session, title } = event {
            published.push((session.0, title));
        }
    }
    assert_eq!(
        published,
        [
            ("current".into(), "Current 🦀".into()),
            ("other".into(), "Other".into())
        ]
    );
    let events: i64 = connection
        .query_row(
            "SELECT count(*) FROM events WHERE kind='session_updated'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(events, 2);
    assert_eq!(starts.len(), 3);
    assert!(
        starts
            .iter()
            .all(|op| !op.ends_with("-deny") && !op.ends_with("-missing"))
    );
    let requests = requests.lock().unwrap();
    for (id, _) in calls {
        assert_eq!(
            requests[1]["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["type"] == "function_call_output" && i["call_id"] == id)
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn tool18_ask_exact_target_and_stale_binding_rechecked_before_intent() {
    let (h, mut generation) = make_harness(allow_all());
    generation
        .permissions
        .insert("opencode_session_rename".into(), Permission::Ask);
    let runtime = runtime_of(&h, generation, vec![]);
    runtime.create_session("current").unwrap();
    runtime.create_session("target").unwrap();
    let (events, _) = tokio::sync::broadcast::channel(32);
    runtime.set_approval_events(&events);
    runtime.register_approval_consumer(false);
    let body = sse_tool_call(
        "stale",
        "opencode_session_rename",
        &json!({"title":"No effect","sessionID":"target"}),
    ) + &sse_completed();
    let (base, _, _) = Fake::start_recording(vec![body, sse_completed()], Duration::ZERO);
    let running = runtime.run_turn(params(
        "current",
        "rename",
        &h,
        provider_of(&base),
        &NO_CANCEL,
    ));
    let approving = async {
        let request = approval_lifecycle::next_request(&runtime).await;
        assert_eq!(request.resources, ["target"]);
        assert_eq!(request.save_patterns, ["target"]);
        assert!(h.db.list_tool_ops("current").unwrap().is_empty());
        h.db.set_pref("tui.session_location.target", "foreign")
            .unwrap();
        runtime
            .reply_approval(oc_core::approval::ApprovalReply {
                id: request.id,
                binding: request.binding,
                decision: oc_core::approval::ApprovalDecision::Once,
            })
            .unwrap();
    };
    let (report, ()) = tokio::join!(running, approving);
    let report = report.unwrap();
    assert_eq!(report.calls[0].state, "failed");
    assert!(h.db.session_meta("target").unwrap().title.is_none());
}

#[tokio::test]
async fn tool18_child_scope_unknown_foreign_and_ask_preintent() {
    for ask in [false, true] {
        let (h, mut generation) = make_harness(allow_all());
        generation.permissions.insert(
            "opencode_session_rename".into(),
            if ask {
                Permission::Ask
            } else {
                Permission::Allow
            },
        );
        let runtime = runtime_of(&h, generation, vec![]);
        runtime.create_session("parent").unwrap();
        runtime.create_session("foreign").unwrap();
        h.db.set_pref("tui.session_location.foreign", "foreign-location")
            .unwrap();
        for child in ["child", "sibling"] {
            h.db.create_child_session("parent", child, Some("helper"), None, None)
                .unwrap();
            h.db.set_pref(&format!("tui.session_location.{child}"), "work")
                .unwrap();
        }
        let targets = if ask {
            vec![None]
        } else {
            vec![
                Some("parent"),
                Some("sibling"),
                Some("foreign"),
                Some("missing"),
                None,
            ]
        };
        let body = targets
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let mut args = json!({"title":"Child own"});
                if let Some(t) = target {
                    args["sessionID"] = json!(t);
                }
                sse_tool_call(&format!("target-{i}"), "opencode_session_rename", &args)
            })
            .collect::<String>()
            + &sse_completed();
        let (base, _, _) = Fake::start_recording(vec![body, sse_completed()], Duration::ZERO);
        let result = runtime
            .run_turn(params(
                "child",
                "rename",
                &h,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await;
        if ask {
            assert!(matches!(result, Err(RuntimeError::ApprovalRequired { .. })));
            assert!(h.db.list_tool_ops("child").unwrap().is_empty());
        } else {
            let report = result.unwrap();
            assert_eq!(
                report
                    .calls
                    .iter()
                    .map(|r| r.state.as_str())
                    .collect::<Vec<_>>(),
                ["failed", "failed", "failed", "failed", "completed"]
            );
        }
        assert_eq!(
            h.db.session_meta("child").unwrap().title.as_deref(),
            if ask { None } else { Some("Child own") }
        );
        for s in ["parent", "sibling", "foreign"] {
            assert!(h.db.session_meta(s).unwrap().title.is_none());
        }
    }
}
