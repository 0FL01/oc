use super::*;
use oc_core::approval::{ApprovalDecision, ApprovalPreview, ApprovalReply};

fn text_harness(permissions: BTreeMap<String, Permission>) -> (Harness, Generation) {
    let (mut harness, generation) = make_harness(permissions);
    let entry = harness.catalog.models["gpt-fixture"].clone();
    harness.catalog.models.insert("text-fixture".into(), entry);
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
async fn external_mutations_auto_admits_patch_edit_and_write_without_saved_grants() {
    for tool in ["apply_patch", "edit", "write"] {
        let mut permissions = allow_all();
        permissions.insert("external_directory".into(), Permission::Ask);
        permissions.insert("apply_patch".into(), Permission::Ask);
        let (harness, generation) = text_harness(permissions);
        let home = tempfile::tempdir().unwrap();
        let target = if tool == "write" {
            home.path().join("new/nested/file")
        } else {
            home.path().join(".zshrc")
        };
        std::fs::write(home.path().join(".zshrc"), "before\n").unwrap();
        let runtime = Runtime::new(
            &harness.db,
            "work",
            generation,
            ProtectedGlobs { patterns: vec![] },
            oc_adapters::files::Files::new(harness._project.path(), harness._data.path()).unwrap(),
            oc_adapters::shell::Shell::new(harness._project.path()).unwrap(),
            BTreeMap::from([("HOME".into(), home.path().to_string_lossy().into_owned())]),
            oc_adapters::tools::ToolRoots {
                project: harness._project.path().into(),
                data: harness._data.path().into(),
            },
            None,
            false,
            DcpConfig::default(),
        )
        .unwrap();
        let (events, mut rx) = tokio::sync::broadcast::channel(64);
        runtime.set_approval_events(&events);
        runtime.register_approval_consumer(true);
        runtime.create_session("external-auto").unwrap();
        let args = match tool {
            "apply_patch" => {
                serde_json::json!({"patchText":"*** Begin Patch\n*** Update File: ~/.zshrc\n@@\n-before\n+after\n*** End Patch"})
            }
            "edit" => serde_json::json!({"path":target,"oldString":"before","newString":"after"}),
            _ => serde_json::json!({"path":target,"content":"after\n"}),
        };
        let (base, _, requests) = Fake::start_recording(
            vec![
                sse_tool_call("external-call", tool, &args) + &sse_completed(),
                sse_completed(),
            ],
            Duration::ZERO,
        );
        let turn = if tool == "apply_patch" {
            params(
                "external-auto",
                "mutate",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            )
        } else {
            text_params("external-auto", &harness, provider_of(&base), &NO_CANCEL)
        };
        let report = runtime.run_turn(turn).await.unwrap();
        assert_eq!(
            report.calls[0].state, "completed",
            "{}",
            report.calls[0].output
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"after\n");
        assert!(runtime.pending_approvals().is_empty());
        let mut asked = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let oc_core::core_app::CoreEvent::PermissionAsked(request) = event {
                asked.push(request);
            }
        }
        assert_eq!(
            asked
                .iter()
                .map(|request| request.action.as_str())
                .collect::<Vec<_>>(),
            ["external_directory", "apply_patch"]
        );
        assert_eq!(
            asked[0].resources,
            [target.parent().unwrap().join("*").to_string_lossy()]
        );
        assert_eq!(asked[1].resources, [target.to_string_lossy()]);
        let records = harness.db.list_tool_ops("external-auto").unwrap();
        assert_eq!(records[0].patch_effects.as_ref().unwrap().total_files, 1);
        let capture = records[0]
            .output_presentation
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .unwrap();
        assert_eq!(capture.state, oc_core::tool_output::CaptureState::Complete);
        assert!(
            std::fs::read_to_string(capture.reference.as_ref().unwrap())
                .unwrap()
                .contains("+after\n")
        );
        let requests = requests.lock().unwrap();
        let original = requests.last().unwrap()["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "function_call" && item["call_id"] == "external-call")
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(original["arguments"].as_str().unwrap())
                .unwrap(),
            args
        );
        let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM permission_grants", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn external_mutations_require_both_permissions_and_rejection_has_no_effects() {
    for mode in [
        "boundary-reject",
        "target-reject",
        "boundary-deny",
        "target-deny",
        "absolute-alias-deny",
        "relative-alias-deny",
    ] {
        let mut permissions = allow_all();
        permissions.insert(
            "external_directory".into(),
            if mode == "boundary-deny" {
                Permission::Deny
            } else {
                Permission::Ask
            },
        );
        permissions.insert(
            "apply_patch".into(),
            if mode == "target-deny" {
                Permission::Deny
            } else {
                Permission::Ask
            },
        );
        let (harness, mut generation) = make_harness(permissions);
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join(".zshrc");
        std::fs::write(&target, "before\n").unwrap();
        let input = if mode.ends_with("alias-deny") {
            generation.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
                &serde_json::json!({"permissions":[
                    {"action":"external_directory","resource":"*","effect":"allow"},
                    {"action":"edit","resource":"*","effect":"allow"},
                    {"action":"edit","resource":target,"effect":"deny"}
                ]}),
            )
            .unwrap();
            let parents = if mode == "absolute-alias-deny" {
                "/../".into()
            } else {
                "../".repeat(harness._project.path().components().count() + 1)
            };
            format!("{parents}{}", target.strip_prefix("/").unwrap().display())
        } else {
            target.to_string_lossy().into_owned()
        };
        let runtime = runtime_of(&harness, generation, vec![]);
        let (events, mut rx) = tokio::sync::broadcast::channel(64);
        runtime.set_approval_events(&events);
        runtime.register_approval_consumer(mode.ends_with("deny"));
        runtime.create_session("external-deny").unwrap();
        let patch = format!(
            "*** Begin Patch\n*** Update File: {}\n@@\n-before\n+after\n*** End Patch",
            input
        );
        let (base, _) = Fake::start(
            vec![
                sse_tool_call(
                    "external-call",
                    "apply_patch",
                    &serde_json::json!({"patchText":patch}),
                ) + &sse_completed(),
                sse_completed(),
            ],
            Duration::ZERO,
        );
        let reply = async {
            if mode.ends_with("reject") {
                let boundary = approval_lifecycle::next_request(&runtime).await;
                assert_eq!(boundary.action, "external_directory");
                assert_eq!(std::fs::read(&target).unwrap(), b"before\n");
                let request = if mode == "target-reject" {
                    runtime
                        .reply_approval(ApprovalReply {
                            id: boundary.id,
                            binding: boundary.binding,
                            decision: ApprovalDecision::Once,
                        })
                        .unwrap();
                    let request = approval_lifecycle::next_request(&runtime).await;
                    assert_eq!(request.action, "apply_patch");
                    assert!(matches!(&request.preview, ApprovalPreview::Patch { .. }));
                    request
                } else {
                    boundary
                };
                assert!(
                    harness
                        .db
                        .list_tool_ops("external-deny")
                        .unwrap()
                        .is_empty()
                );
                runtime
                    .reply_approval(ApprovalReply {
                        id: request.id,
                        binding: request.binding,
                        decision: ApprovalDecision::Reject { feedback: None },
                    })
                    .unwrap();
            }
        };
        let (report, ()) = tokio::join!(
            runtime.run_turn(params(
                "external-deny",
                "mutate",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            )),
            reply
        );
        assert_ne!(report.unwrap().calls[0].state, "completed");
        assert_eq!(std::fs::read(&target).unwrap(), b"before\n");
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
        assert!(runtime.pending_approvals().is_empty());
        let mut asked = 0;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, oc_core::core_app::CoreEvent::PermissionAsked(_)) {
                asked += 1;
            }
        }
        assert_eq!(
            asked,
            if mode == "target-reject" {
                2
            } else {
                usize::from(mode == "boundary-reject")
            }
        );
    }
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
            let write_preview = rows[0]
                .output_presentation
                .as_ref()
                .and_then(|presentation| presentation.write.as_ref());
            if tool == "write" && action == "allow" {
                let preview = write_preview.unwrap();
                assert_eq!(preview.content, "after\n");
                assert_eq!(preview.content_bytes, 6);
                assert!(!preview.content_limited && !preview.path_limited);
            } else {
                assert!(write_preview.is_none());
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
        assert_eq!(old.output_presentation, new.output_presentation);
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
    let preview = current[1]
        .output_presentation
        .as_ref()
        .unwrap()
        .write
        .as_ref()
        .unwrap();
    assert_eq!(preview.content, "new\n");
    assert!(!preview.content_limited);
    assert!(!preview.content.contains("present-day"));
    assert!(
        current[0]
            .output_presentation
            .as_ref()
            .unwrap()
            .write
            .is_none()
    );
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
