//! TOOL19 common admission observes real prepared-generation and intent barriers.
use super::*;
use oc_core::approval::{ApprovalDecision, ApprovalReply};
use serde_json::json;

fn destination() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("opencode.json"),json!({"model":"fixture/m","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"m":{}}}}}).to_string()).unwrap();
    dir
}

#[tokio::test]
async fn tool19_ask_exact_target_directory_generation_refresh_and_no_intent_or_grant() {
    let (h, mut generation) = make_harness(allow_all());
    let dir = destination();
    generation
        .permissions
        .insert("opencode_session_move".into(), Permission::Ask);
    generation
        .permissions
        .insert("external_directory".into(), Permission::Allow);
    generation.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
        &json!({"permission":{"opencode_session_move":"ask","external_directory":"allow"}}),
    )
    .unwrap();
    let runtime = runtime_of(&h, generation, vec![]);
    runtime.create_session("caller").unwrap();
    runtime.create_session("target").unwrap();
    let (events, _rx) = tokio::sync::broadcast::channel(32);
    runtime.set_approval_events(&events);
    runtime.register_approval_consumer(false);
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call(
                "move",
                "opencode_session_move",
                &json!({"directory":dir.path(),"sessionID":"target"}),
            ) + &sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut starts = 0;
    let turn = runtime.run_turn_with_tool_events(
        params("caller", "move", &h, provider_of(&base), &NO_CANCEL),
        |_| {},
        |_, _| {},
        |_, _| {},
        |_, event| {
            if matches!(event, ToolCallEvent::Started { .. }) {
                starts += 1;
            }
        },
    );
    let answer = async {
        let request = approval_lifecycle::next_request(&runtime).await;
        assert_eq!(
            request.resources,
            [
                "target".to_string(),
                dir.path().to_string_lossy().into_owned()
            ]
        );
        assert!(request.save_patterns.is_empty());
        assert!(h.db.list_tool_ops("caller").unwrap().is_empty());
        assert!(
            runtime
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: request.binding.clone(),
                    decision: ApprovalDecision::Always
                })
                .is_err()
        );
        std::fs::write(dir.path().join("AGENTS.md"), "changed during approval").unwrap();
        runtime
            .reply_approval(ApprovalReply {
                id: request.id,
                binding: request.binding,
                decision: ApprovalDecision::Once,
            })
            .unwrap();
    };
    let (report, ()) = tokio::join!(turn, answer);
    let report = report.unwrap();
    assert_eq!(starts, 0);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].state, "failed");
    assert!(report.calls[0].output.contains("generation changed"));
    assert_eq!(
        h.db.get_pref("tui.session_location.target")
            .unwrap()
            .as_deref(),
        Some("work")
    );
    let wire = requests.lock().unwrap();
    assert_eq!(
        wire[1]["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["type"] == "function_call_output" && i["call_id"] == "move")
            .count(),
        1
    );
}

#[tokio::test]
async fn tool19_missing_foreign_busy_family_structural_guards_before_intent() {
    let (h, mut generation) = make_harness(allow_all());
    let dir = destination();
    generation
        .permissions
        .insert("opencode_session_move".into(), Permission::Allow);
    generation
        .permissions
        .insert("external_directory".into(), Permission::Allow);
    let runtime = runtime_of(&h, generation, vec![]);
    for id in ["caller", "busy", "foreign"] {
        runtime.create_session(id).unwrap();
    }
    h.db.create_child_session("busy", "busy-child", None, None, None)
        .unwrap();
    h.db.begin_turn("held-child", "busy-child", "held").unwrap();
    h.db.set_pref("tui.session_location.foreign", "/foreign")
        .unwrap();
    let calls = [
        ("missing", "missing"),
        ("foreign", "foreign"),
        ("busy", "busy"),
    ];
    let body = calls
        .iter()
        .map(|(id, target)| {
            sse_tool_call(
                id,
                "opencode_session_move",
                &json!({"directory":dir.path(),"sessionID":target}),
            )
        })
        .collect::<String>()
        + &sse_completed();
    let (base, _) = Fake::start(vec![body, sse_completed()], Duration::ZERO);
    let mut starts = 0;
    let report = runtime
        .run_turn_with_tool_events(
            params("caller", "move", &h, provider_of(&base), &NO_CANCEL),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if matches!(event, ToolCallEvent::Started { .. }) {
                    starts += 1;
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(starts, 0);
    assert_eq!(
        report
            .calls
            .iter()
            .map(|c| c.state.as_str())
            .collect::<Vec<_>>(),
        ["failed", "failed", "failed"]
    );
    assert_eq!(
        h.db.get_pref("tui.session_location.busy")
            .unwrap()
            .as_deref(),
        Some("work")
    );
}
