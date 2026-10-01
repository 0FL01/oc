use super::*;
use oc_core::approval::{ApprovalDecision, ApprovalPreview, ApprovalReply};

fn text_harness(permissions: BTreeMap<String, Permission>) -> (Harness, Generation) {
    let (mut harness, generation) = make_harness(permissions);
    harness.catalog.models.insert(
        "text-fixture".into(),
        harness.catalog.models["gpt-fixture"].clone(),
    );
    (harness, generation)
}
fn text_params<'a>(
    session: &str,
    harness: &'a Harness,
    provider: ResponsesConfig,
    cancel: &'a AtomicBool,
) -> TurnParams<'a> {
    let mut turn = params(session, "mutate", harness, provider, cancel);
    turn.model_id = "text-fixture".into();
    turn
}

#[tokio::test]
async fn tool20_ask_preview_changed_preimage_cancel_and_durable_effects() {
    for tool in ["edit", "write"] {
        for action in ["allow", "stale", "cancel"] {
            let mut permissions = allow_all();
            permissions.insert("apply_patch".into(), Permission::Ask);
            let (harness, generation) = text_harness(permissions);
            let path = harness._project.path().join("file");
            std::fs::write(&path, "before\n").unwrap();
            let runtime = runtime_of(&harness, generation, vec![]);
            runtime.create_session("files").unwrap();
            let (events, _rx) = tokio::sync::broadcast::channel(64);
            runtime.set_approval_events(&events);
            runtime.register_approval_consumer(false);
            let args = if tool == "write" {
                serde_json::json!({"path":"file","content":"after\n"})
            } else {
                serde_json::json!({"path":"file","oldString":"before","newString":"after"})
            };
            let (base, _) = Fake::start(
                vec![
                    sse_tool_call("file-call", tool, &args) + &sse_completed(),
                    sse_completed(),
                ],
                Duration::ZERO,
            );
            let cancel = AtomicBool::new(false);
            let reply = async {
                let request = approval_lifecycle::next_request(&runtime).await;
                assert_eq!(request.action, "apply_patch");
                assert_eq!(request.resources, ["file"]);
                assert!(
                    matches!(&request.preview,ApprovalPreview::Patch { files,.. } if files.len()==1 && files[0].additions==1 && files[0].deletions==1)
                );
                assert_eq!(std::fs::read(&path).unwrap(), b"before\n");
                assert!(harness.db.list_tool_ops("files").unwrap().is_empty());
                if action == "stale" {
                    std::fs::write(&path, "foreign\n").unwrap();
                }
                if action == "cancel" {
                    cancel.store(true, Ordering::Release);
                } else {
                    runtime
                        .reply_approval(ApprovalReply {
                            id: request.id,
                            binding: request.binding,
                            decision: ApprovalDecision::Once,
                        })
                        .unwrap();
                }
            };
            let (report, ()) = tokio::join!(
                runtime.run_turn(text_params("files", &harness, provider_of(&base), &cancel)),
                reply
            );
            let report = report.unwrap();
            assert_eq!(
                report.calls[0].state,
                if action == "allow" {
                    "completed"
                } else if action == "stale" {
                    "failed"
                } else {
                    "cancelled"
                }
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                if action == "allow" {
                    b"after\n".as_slice()
                } else if action == "stale" {
                    b"foreign\n".as_slice()
                } else {
                    b"before\n".as_slice()
                }
            );
            let rows = harness.db.list_tool_ops("files").unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, tool);
            if action == "allow" {
                assert_eq!(rows[0].patch_effects.as_ref().unwrap().total_files, 1);
            }
        }
    }
}

#[tokio::test]
async fn tool12_excluded_call_fails_paired_without_effects() {
    let (harness, generation) = text_harness(allow_all());
    let runtime = runtime_of(&harness, generation, vec![]);
    runtime.create_session("excluded").unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call(
                "excluded-call",
                "apply_patch",
                &serde_json::json!({"patchText":"*** Begin Patch\n*** Add File: excluded\n+bad\n*** End Patch"}),
            ) + &sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(text_params(
            "excluded",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.calls[0].state, "failed");
    assert!(!harness._project.path().join("excluded").exists());
    let rows = harness.db.list_tool_ops("excluded").unwrap();
    assert_eq!(rows.len(), 1);
    let requests = requests.lock().unwrap();
    assert!(
        requests[1]["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["call_id"] == "excluded-call" && i["type"] == "function_call_output")
    );
}
#[tokio::test]
async fn tool20_allow_rechecks_prepared_bytes_after_durable_started_event() {
    let (harness, generation) = text_harness(allow_all());
    let path = harness._project.path().join("file");
    std::fs::write(&path, b"before\n").unwrap();
    let runtime = runtime_of(&harness, generation, vec![]);
    runtime.create_session("allow").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "write",
                "write",
                &serde_json::json!({"path":"file","content":"after\n"}),
            ) + &sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut started = false;
    let report = runtime
        .run_turn_with_tool_events(
            text_params("allow", &harness, provider_of(&base), &NO_CANCEL),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if matches!(event,ToolCallEvent::Started { name,.. } if name=="write") {
                    assert_eq!(
                        harness.db.list_tool_ops("allow").unwrap()[0].state,
                        "started"
                    );
                    std::fs::write(&path, b"foreign\n").unwrap();
                    started = true;
                }
            },
        )
        .await
        .unwrap();
    assert!(started);
    assert_eq!(report.calls[0].state, "failed");
    assert_eq!(std::fs::read(path).unwrap(), b"foreign\n");
    assert!(
        harness.db.list_tool_ops("allow").unwrap()[0]
            .patch_effects
            .as_ref()
            .unwrap()
            .files
            .is_empty()
    );
}

#[tokio::test]
async fn tool20_saved_write_grant_uses_shared_action_and_exact_path_after_reopen() {
    let mut permissions = allow_all();
    permissions.insert("apply_patch".into(), Permission::Ask);
    let (harness, generation) = text_harness(permissions);
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "write",
                "write",
                &serde_json::json!({"path":"file","content":"before\n"}),
            ) + &sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    {
        let runtime = runtime_of(&harness, generation.clone(), vec![]);
        runtime.create_session("grant").unwrap();
        let (events, _rx) = tokio::sync::broadcast::channel(64);
        runtime.set_approval_events(&events);
        runtime.register_approval_consumer(false);
        let reply = async {
            let request = approval_lifecycle::next_request(&runtime).await;
            assert_eq!(request.action, "apply_patch");
            assert_eq!(request.save_patterns, ["file"]);
            runtime
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: request.binding,
                    decision: ApprovalDecision::Always,
                })
                .unwrap();
        };
        let (report, ()) = tokio::join!(
            runtime.run_turn(text_params(
                "grant",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            )),
            reply
        );
        assert_eq!(report.unwrap().calls[0].state, "completed");
    }
    let runtime = runtime_of(&harness, generation, vec![]);
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "edit",
                "edit",
                &serde_json::json!({"path":"file","oldString":"before","newString":"after"}),
            ) + &sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    assert_eq!(
        runtime
            .run_turn(text_params(
                "grant",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await
            .unwrap()
            .calls[0]
            .state,
        "completed"
    );
    assert_eq!(
        std::fs::read(harness._project.path().join("file")).unwrap(),
        b"after\n"
    );
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "other",
                "write",
                &serde_json::json!({"path":"other","content":"unapproved"}),
            ) + &sse_completed(),
        ],
        Duration::ZERO,
    );
    assert!(matches!(
        runtime
            .run_turn(text_params(
                "grant",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await,
        Err(RuntimeError::ApprovalRequired { .. })
    ));
    assert!(!harness._project.path().join("other").exists());
}
#[tokio::test]
async fn tool12_next_turn_and_reopen_switch_family_keep_historical_calls_verbatim() {
    let (harness, generation) = text_harness(allow_all());
    let patch =
        serde_json::json!({"patchText":"*** Begin Patch\n*** Add File: file\n+old\n*** End Patch"});
    let write = serde_json::json!({"path":"file","content":"new\n"});
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call("old-patch", "apply_patch", &patch) + &sse_completed(),
            sse_completed(),
            sse_tool_call("new-write", "write", &write) + &sse_completed(),
            sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    {
        let runtime = runtime_of(&harness, generation.clone(), vec![]);
        runtime.create_session("family").unwrap();
        assert_eq!(
            runtime
                .run_turn(params(
                    "family",
                    "first",
                    &harness,
                    provider_of(&base),
                    &NO_CANCEL
                ))
                .await
                .unwrap()
                .calls[0]
                .state,
            "completed"
        );
        assert_eq!(
            runtime
                .run_turn(text_params(
                    "family",
                    &harness,
                    provider_of(&base),
                    &NO_CANCEL
                ))
                .await
                .unwrap()
                .calls[0]
                .state,
            "completed"
        );
    }
    let original = harness.db.list_tool_ops("family").unwrap();
    std::fs::write(harness._project.path().join("file"), b"present-day\n").unwrap();
    let runtime = runtime_of(&harness, generation, vec![]);
    assert_eq!(
        runtime
            .run_turn(params(
                "family",
                "third",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await
            .unwrap()
            .status,
        TurnStatus::Completed
    );
    let current = harness.db.list_tool_ops("family").unwrap();
    assert_eq!(original.len(), 2);
    assert_eq!(current.len(), 2);
    for (old, new) in original.iter().zip(&current) {
        assert_eq!(old.input, new.input);
        assert_eq!(old.output, new.output);
        assert_eq!(old.patch_effects, new.patch_effects);
    }
    assert_eq!(
        std::fs::read(harness._project.path().join("file")).unwrap(),
        b"present-day\n"
    );
    let wire = requests.lock().unwrap();
    for (index, expected) in [
        (0, vec!["apply_patch"]),
        (2, vec!["write", "edit"]),
        (4, vec!["apply_patch"]),
    ] {
        let names: Vec<_> = wire[index]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .filter(|name| matches!(*name, "apply_patch" | "edit" | "write"))
            .collect();
        assert_eq!(names, expected);
    }
    // Cross-model provider projection keeps its existing public-only binding
    // rule. The raw operation graph is immutable; the issuing model's immediate
    // followup still receives its exact matched native call/result.
    assert_eq!(current[0].name, "apply_patch");
    assert_eq!(
        current[0].input.as_deref(),
        Some(patch.to_string().as_str())
    );
    assert_eq!(current[1].name, "write");
    assert_eq!(
        current[1].input.as_deref(),
        Some(write.to_string().as_str())
    );
    {
        let (id, name, args) = ("new-write", "write", write);
        let arguments = args.to_string();
        assert!(
            wire[3]["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "function_call"
                    && item["call_id"] == id
                    && item["name"] == name
                    && item["arguments"].as_str() == Some(arguments.as_str()))
        );
        assert!(
            wire[3]["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == id)
        );
    }
}
