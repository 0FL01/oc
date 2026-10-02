use super::*;
use oc_core::approval::{ApprovalDecision, ApprovalPreview, ApprovalReply, ApprovalRequest};

pub(super) async fn next_request(runtime: &Runtime<'_>) -> ApprovalRequest {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let Some(request) = runtime.pending_approvals().into_iter().next() {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("real pending request")
}
fn answer(runtime: &Runtime<'_>, request: ApprovalRequest, decision: ApprovalDecision) {
    runtime
        .reply_approval(ApprovalReply {
            id: request.id,
            binding: request.binding,
            decision,
        })
        .unwrap();
}
fn consumer(
    runtime: &Runtime<'_>,
) -> tokio::sync::broadcast::Receiver<oc_core::core_app::CoreEvent> {
    let (tx, rx) = tokio::sync::broadcast::channel(64);
    runtime.set_approval_events(&tx);
    runtime.register_approval_consumer(false);
    rx
}

#[tokio::test]
async fn external_read_shared_boundary_and_action_admission_emit_only_genuine_asks() {
    use oc_adapters::permissions::PermissionRules;
    use oc_core::core_app::CoreEvent;
    for mode in [
        "allow",
        "boundary-ask",
        "read-ask",
        "source-deny",
        "profile-deny",
        "boundary-reject",
        "boundary-cancel",
    ] {
        let asks = mode.ends_with("ask") || mode.ends_with("reject") || mode.ends_with("cancel");
        let mut permissions = allow_all();
        permissions.insert("external_directory".into(), Permission::Allow);
        let (harness, mut generation) = make_harness(permissions);
        let cache = tempfile::tempdir().unwrap();
        let file = cache.path().join("fact.txt");
        std::fs::write(&file, "before approval").unwrap();
        generation.permission_rules = PermissionRules::from_config(&serde_json::json!({"permissions":[
            {"action":"read","resource":"*","effect":if mode=="read-ask" {"ask"} else {"allow"}},
            {"action":"external_directory","resource":"*","effect":"deny"},
            {"action":"external_directory","resource":cache.path().join("*"),"effect":if asks && mode!="read-ask" {"ask"} else {"allow"}}
        ]})).unwrap();
        let ceiling = PermissionRules::from_config(
            &serde_json::json!({"permission":{"external_directory":"deny"}}),
        )
        .unwrap();
        if mode == "source-deny" {
            generation.permission_rules.extend(ceiling);
        } else if mode == "profile-deny" {
            generation
                .permission_rules
                .narrow(&generation.permissions, &BTreeMap::new(), &ceiling);
        }
        let runtime = runtime_of(&harness, generation, vec![]);
        let mut events = consumer(&runtime);
        let cancel = AtomicBool::new(false);
        runtime.create_session("external").unwrap();
        let (base, _) = Fake::start(
            vec![
                sse_tool_call("external", "read", &serde_json::json!({"path":file}))
                    + &sse_completed(),
                sse_delta("done") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let reply = async {
            if asks {
                let request = next_request(&runtime).await;
                assert!(harness.db.list_tool_ops("external").unwrap().is_empty());
                assert_eq!(
                    request.action,
                    if mode != "read-ask" {
                        "external_directory"
                    } else {
                        "read"
                    }
                );
                assert_eq!(
                    request.resources,
                    [if mode != "read-ask" {
                        cache.path().join("*")
                    } else {
                        file.clone()
                    }
                    .to_string_lossy()]
                );
                if mode != "read-ask" {
                    assert!(request.save_patterns.is_empty());
                }
                std::fs::write(&file, "AFTER_SHARED_CONSUMER").unwrap();
                if mode.ends_with("cancel") {
                    cancel.store(true, Ordering::Release);
                    return;
                }
                answer(
                    &runtime,
                    request,
                    if mode.ends_with("reject") {
                        ApprovalDecision::Reject { feedback: None }
                    } else {
                        ApprovalDecision::Once
                    },
                );
            }
        };
        let (report, ()) = tokio::join!(
            runtime.run_turn(params(
                "external",
                "invoke",
                &harness,
                provider_of(&base),
                &cancel
            )),
            reply
        );
        let report = report.unwrap();
        if mode.ends_with("reject") || mode.ends_with("cancel") {
            assert_eq!(report.status, TurnStatus::Cancelled);
            assert_eq!(
                report.calls[0].state,
                if mode.ends_with("reject") {
                    "denied"
                } else {
                    "cancelled"
                }
            );
            assert!(!report.calls[0].output.contains("AFTER_SHARED_CONSUMER"));
        } else if mode.ends_with("deny") {
            assert!(report.calls[0].output.contains("denied"));
            assert!(!report.calls[0].output.contains("before approval"));
        } else {
            assert_eq!(report.calls[0].state, "completed");
            assert!(report.calls[0].output.contains(if mode == "allow" {
                "before approval"
            } else {
                "AFTER_SHARED_CONSUMER"
            }));
        }
        let mut asked = 0;
        while let Ok(event) = events.try_recv() {
            if matches!(event, CoreEvent::PermissionAsked(_)) {
                asked += 1;
            }
        }
        assert_eq!(asked, usize::from(asks), "{mode}");
        let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn approval_webfetch_hostname_does_not_resolve_before_owner_reply_or_without_consumer() {
    for registered in [false, true] {
        let mut permissions = allow_all();
        permissions.insert("webfetch".into(), Permission::Ask);
        let (harness, generation) = make_harness(permissions);
        let runtime = runtime_of(&harness, generation, vec![]);
        runtime.create_session("dns").unwrap();
        let _events = registered.then(|| consumer(&runtime));
        let url = "https://permission-before-dns.invalid/resource";
        let (base, _) = Fake::start(
            vec![
                sse_tool_call(
                    "dns",
                    "webfetch",
                    &serde_json::json!({"url":url,"format":"text"}),
                ) + &sse_completed(),
                sse_delta("rejected") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let replies = async {
            if registered {
                let request = next_request(&runtime).await;
                assert_eq!(request.resources, vec![url]);
                assert!(harness.db.list_tool_ops("dns").unwrap().is_empty());
                answer(
                    &runtime,
                    request,
                    ApprovalDecision::Reject { feedback: None },
                );
            }
        };
        let (report, ()) = tokio::join!(
            runtime.run_turn(params(
                "dns",
                "fetch",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            )),
            replies
        );
        if registered {
            assert_eq!(report.unwrap().status, TurnStatus::Cancelled);
        } else {
            assert!(matches!(report, Err(RuntimeError::ApprovalRequired { .. })));
        }
    }
}

#[tokio::test]
async fn approval_shell_started_rename_executes_only_in_opened_cwd() {
    for (symlink_replacement, canonical) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut permissions = allow_all();
        permissions.insert("bash".into(), Permission::Ask);
        let (harness, generation) = make_harness(permissions);
        let cwd = harness._project.path().join("cwd");
        let retained = harness._project.path().join("retained");
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(&cwd).unwrap();
        let runtime = runtime_of(&harness, generation, vec![]);
        let _events = consumer(&runtime);
        runtime.create_session("pinned").unwrap();
        let (base, _) = Fake::start(
            vec![
                sse_tool_call(
                    "cwd",
                    if canonical { "shell" } else { "bash" },
                    &if canonical {
                        serde_json::json!({"command":"touch marker", "workdir":"cwd"})
                    } else {
                        serde_json::json!({"argv":["/usr/bin/touch","marker"],"cwd":"cwd"})
                    },
                ) + &sse_completed(),
                sse_delta("done") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let mut started = false;
        let running = runtime.run_turn_with_tool_events(
            params("pinned", "invoke", &harness, provider_of(&base), &NO_CANCEL),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if matches!(event, ToolCallEvent::Started { name, .. } if name == if canonical { "shell" } else { "bash" }) {
                    assert!(!started);
                    started = true;
                    std::fs::rename(&cwd, &retained).unwrap();
                    if symlink_replacement {
                        std::os::unix::fs::symlink(outside.path(), &cwd).unwrap();
                    } else {
                        std::fs::create_dir(&cwd).unwrap();
                    }
                }
            },
        );
        let reply = async {
            answer(
                &runtime,
                next_request(&runtime).await,
                ApprovalDecision::Once,
            );
        };
        let (report, ()) = tokio::join!(running, reply);
        assert_eq!(report.unwrap().calls[0].state, "completed");
        assert!(started);
        assert!(retained.join("marker").exists());
        assert!(!cwd.join("marker").exists());
        assert!(!outside.path().join("marker").exists());
    }
}

#[tokio::test]
async fn tool12_command_always_reopen_does_not_inherit_argv_glob_or_widen_alias_deny() {
    let mut permissions = allow_all();
    permissions.insert("bash".into(), Permission::Ask);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation.clone(), vec![]);
    let _events = consumer(&runtime);
    runtime.create_session("command-grant").unwrap();
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    // Simulate an existing donor-compatible argv wildcard grant, retained in
    // the real native store. It cannot silently authorize command source text.
    conn.execute(
        "INSERT INTO permission_grants VALUES(?1,'bash','*')",
        ["irrelevant-project"],
    )
    .unwrap();
    let command = "printf approved >> marker";
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("command", "shell", &serde_json::json!({"command":command}))
                + &sse_tool_call(
                    "exact",
                    "shell",
                    &serde_json::json!({"command":"touch approved-exact"}),
                )
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let reply = async {
        let request = next_request(&runtime).await;
        assert_eq!(request.action, "bash");
        assert_eq!(request.resources, [command]);
        assert!(
            matches!(&request.preview, ApprovalPreview::Shell { command: preview, .. } if preview == command)
        );
        assert!(
            harness
                .db
                .list_tool_ops("command-grant")
                .unwrap()
                .is_empty()
        );
        answer(&runtime, request, ApprovalDecision::Always);
        let request = next_request(&runtime).await;
        assert_eq!(request.resources, ["touch approved-exact"]);
        answer(&runtime, request, ApprovalDecision::Always);
    };
    let (report, ()) = tokio::join!(
        runtime.run_turn(params(
            "command-grant",
            "invoke",
            &harness,
            provider_of(&base),
            &NO_CANCEL
        )),
        reply
    );
    assert_eq!(report.unwrap().calls[0].state, "completed");
    let project: String = conn
        .query_row(
            "SELECT project FROM permission_grants WHERE project != 'irrelevant-project'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(runtime);
    let runtime = runtime_of(&harness, generation.clone(), vec![]);
    // Exactly the same resource spelling remains a different interpretation
    // domain: canonical Always cannot authorize literal argv.
    std::fs::remove_file(harness._project.path().join("approved-exact")).unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "argv-exact",
                "bash",
                &serde_json::json!({"argv":["touch","approved-exact"]}),
            ) + &sse_completed(),
        ],
        Duration::ZERO,
    );
    assert!(matches!(
        runtime
            .run_turn(params(
                "command-grant",
                "legacy",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await,
        Err(RuntimeError::ApprovalRequired { .. })
    ));
    assert!(!harness._project.path().join("approved-exact").exists());
    conn.execute(
        "INSERT INTO permission_grants VALUES(?1,'bash','*')",
        [&project],
    )
    .unwrap();
    // Reopened owner has no approval consumer. Exact saved command succeeds.
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("same", "shell", &serde_json::json!({"command":command}))
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    assert_eq!(
        runtime
            .run_turn(params(
                "command-grant",
                "same",
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
    // Legacy glob still cannot grant a different shell source command.
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "different",
                "shell",
                &serde_json::json!({"command":"touch wrong-marker"}),
            ) + &sse_completed(),
        ],
        Duration::ZERO,
    );
    assert!(matches!(
        runtime
            .run_turn(params(
                "command-grant",
                "different",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await,
        Err(RuntimeError::ApprovalRequired { .. })
    ));
    assert!(!harness._project.path().join("wrong-marker").exists());
    drop(runtime);
    let mut denied = generation;
    denied.permissions.insert("shell".into(), Permission::Deny);
    let runtime = runtime_of(&harness, denied, vec![]);
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("denied", "shell", &serde_json::json!({"command":command}))
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    assert!(
        runtime
            .run_turn(params(
                "command-grant",
                "denied",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await
            .unwrap()
            .calls[0]
            .output
            .contains("denied")
    );
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("marker")).unwrap(),
        "approvedapproved"
    );
}

#[tokio::test]
async fn tool13_invalid_shell_never_commits_execution_intent() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, vec![]);
    runtime.create_session("invalid-shell").unwrap();
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    conn.execute_batch("CREATE TRIGGER no_invalid_shell_intent BEFORE INSERT ON tool_operations WHEN NEW.state='started' BEGIN SELECT RAISE(FAIL,'invalid shell execution intent'); END;").unwrap();
    for args in [
        serde_json::json!({"command":"touch marker","background":"yes"}),
        serde_json::json!({"command":"touch marker","timeout":-1}),
        serde_json::json!({"command":"touch marker","workdir":"../"}),
        serde_json::json!({"command":"touch marker","timeout":600001}),
        serde_json::json!({"command":"touch marker","argv":["true"]}),
    ] {
        let (base, _) = Fake::start(
            vec![
                sse_tool_call("invalid", "shell", &args) + &sse_completed(),
                sse_delta("done") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let report = runtime
            .run_turn(params(
                "invalid-shell",
                "invalid",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(report.calls[0].state, "failed");
        assert!(!harness._project.path().join("marker").exists());
    }
}

#[tokio::test]
async fn approval_compress_prepared_plan_waits_then_commits_after_once() {
    let mut permissions = allow_all();
    permissions.insert("compress".into(), Permission::Ask);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation, vec![]);
    let _events = consumer(&runtime);
    runtime.create_session("plan").unwrap();
    for _ in 0..2 {
        harness
            .db
            .append_message("plan", "assistant", &"old content ".repeat(500))
            .unwrap();
    }
    let history = harness.db.read_history_full("plan").unwrap();
    let args = serde_json::json!({"topic":"old","content":[{"startId":history[0].0,"endId":history[1].0,"summary":"old content summary"}]});
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("plan", "compress", &args) + &sse_completed(),
            sse_delta("compressed") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let reply = async {
        let request = next_request(&runtime).await;
        assert!(harness.db.list_tool_ops("plan").unwrap().is_empty());
        assert!(
            oc_adapters::dcp::load_blocks(&harness.db, "plan")
                .unwrap()
                .is_empty()
        );
        answer(&runtime, request, ApprovalDecision::Once);
    };
    let (report, ()) = tokio::join!(
        runtime.run_turn(params(
            "plan",
            "compress",
            &harness,
            provider_of(&base),
            &NO_CANCEL
        )),
        reply
    );
    assert_eq!(report.unwrap().calls[0].state, "completed");
    assert_eq!(
        oc_adapters::dcp::load_blocks(&harness.db, "plan")
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn approval_structural_compress_never_asks_saves_or_starts() {
    for (enabled, manual, foreign) in [
        (false, false, false),
        (true, true, false),
        (true, false, false),
        (true, false, true),
    ] {
        let mut permissions = allow_all();
        permissions.insert("compress".into(), Permission::Ask);
        let (harness, generation) = make_harness(permissions);
        let runtime = runtime_with_dcp(
            &harness,
            generation,
            vec![],
            DcpConfig {
                enabled,
                manual_mode: manual,
                ..DcpConfig::default()
            },
        );
        let mut events = consumer(&runtime);
        runtime.create_session("compress").unwrap();
        runtime.create_session("foreign").unwrap();
        harness
            .db
            .append_message("foreign", "assistant", "foreign content")
            .unwrap();
        let id = if foreign {
            harness.db.read_history_full("foreign").unwrap()[0]
                .0
                .clone()
        } else {
            "m999999".into()
        };
        let args = serde_json::json!({"topic":"invalid","content":[{"startId":id,"endId":id,"summary":"must not commit"}]});
        let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
        conn.execute_batch("CREATE TRIGGER no_invalid_compress_intent BEFORE INSERT ON tool_operations WHEN NEW.state='started' BEGIN SELECT RAISE(FAIL,'invalid compression intent'); END;").unwrap();
        let (base, _) = Fake::start(
            vec![
                sse_tool_call("invalid", "compress", &args) + &sse_completed(),
                sse_delta("recovered") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let report = tokio::time::timeout(
            Duration::from_secs(5),
            runtime.run_turn(params(
                "compress",
                "test",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            )),
        )
        .await
        .expect("structural failure must not wait")
        .unwrap();
        assert_eq!(report.calls[0].state, "failed");
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(
                event,
                oc_core::core_app::CoreEvent::PermissionAsked(_)
                    | oc_core::core_app::CoreEvent::PermissionResolved { .. }
            ));
        }
        assert_eq!(
            conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(
            oc_adapters::dcp::load_blocks(&harness.db, "compress")
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn approval_saved_literal_backslash_does_not_approve_distinct_unix_path() {
    let mut permissions = allow_all();
    permissions.insert("read".into(), Permission::Ask);
    let (harness, generation) = make_harness(permissions);
    std::fs::write(harness._project.path().join(r"a\b"), "literal").unwrap();
    std::fs::create_dir(harness._project.path().join("a")).unwrap();
    std::fs::write(harness._project.path().join("a/b"), "separate").unwrap();
    let runtime = runtime_of(&harness, generation, vec![]);
    let _events = consumer(&runtime);
    runtime.create_session("literal").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("literal", "read", &serde_json::json!({"path":r"a\b"}))
                + &sse_tool_call("separator", "read", &serde_json::json!({"path":"a/b"}))
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let replies = async {
        let literal = next_request(&runtime).await;
        assert_eq!(literal.resources, vec![r"a\b"]);
        assert_eq!(literal.save_patterns, vec![r"a\b"]);
        answer(&runtime, literal, ApprovalDecision::Always);
        let separate = next_request(&runtime).await;
        assert_eq!(separate.resources, vec!["a/b"]);
        answer(
            &runtime,
            separate,
            ApprovalDecision::Reject {
                feedback: Some("different file".into()),
            },
        );
    };
    let (report, ()) = tokio::join!(
        runtime.run_turn(params(
            "literal",
            "read",
            &harness,
            provider_of(&base),
            &NO_CANCEL
        )),
        replies
    );
    let report = report.unwrap();
    assert!(report.calls[0].output.contains("literal"));
    assert_eq!(report.calls[1].state, "denied");
    assert!(!report.calls[1].output.contains("separate"));
}

#[tokio::test]
async fn approval_replaced_resolved_shell_cwd_is_stale_before_intent() {
    let mut permissions = allow_all();
    permissions.insert("bash".into(), Permission::Ask);
    let (harness, generation) = make_harness(permissions);
    let cwd = harness._project.path().join("cwd");
    std::fs::create_dir(&cwd).unwrap();
    let runtime = runtime_of(&harness, generation, vec![]);
    let mut events = consumer(&runtime);
    runtime.create_session("cwd").unwrap();
    let conn = rusqlite::Connection::open(harness._data.path().join("oc.sqlite")).unwrap();
    conn.execute_batch("CREATE TRIGGER forbid_stale_cwd_intent BEFORE INSERT ON tool_operations WHEN NEW.state='started' BEGIN SELECT RAISE(FAIL,'stale cwd execution intent'); END;").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "cwd",
                "bash",
                &serde_json::json!({"argv":["/usr/bin/touch","marker"],"cwd":"cwd"}),
            ) + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let reply = async {
        let request = next_request(&runtime).await;
        assert!(
            matches!(&request.preview, ApprovalPreview::Shell { cwd: displayed, .. } if displayed == &cwd.display().to_string())
        );
        assert!(harness.db.list_tool_ops("cwd").unwrap().is_empty());
        std::fs::rename(&cwd, harness._project.path().join("old-cwd")).unwrap();
        std::fs::create_dir(&cwd).unwrap();
        answer(&runtime, request, ApprovalDecision::Once);
    };
    let (report, ()) = tokio::join!(
        runtime.run_turn(params(
            "cwd",
            "invoke",
            &harness,
            provider_of(&base),
            &NO_CANCEL
        )),
        reply
    );
    assert!(
        report.unwrap().calls[0]
            .output
            .contains("approval prerequisites changed")
    );
    assert!(!cwd.join("marker").exists());
    assert!(!harness._project.path().join("old-cwd/marker").exists());
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(
            event,
            oc_core::core_app::CoreEvent::ToolCallStarted { .. }
        ));
    }
    assert!(
        harness
            .db
            .list_tool_ops("cwd")
            .unwrap()
            .iter()
            .all(|op| op.state != "started")
    );
}

#[tokio::test]
async fn approval_real_owner_queries_reconnect_and_replies_while_provider_turn_waits() {
    use oc_core::{core_app::CoreEvent, domain::SessionId};
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "owner",
                "bash",
                &serde_json::json!({"argv":["/usr/bin/touch","owner-marker"]}),
            ) + &sse_completed(),
            sse_delta("finished") + &sse_completed(),
            sse_tool_call(
                "cancel-owner",
                "bash",
                &serde_json::json!({"argv":["/usr/bin/touch","cancel-owner-marker"]}),
            ) + &sse_completed(),
        ],
        Duration::ZERO,
    );
    std::fs::write(project.path().join("opencode.json"), serde_json::json!({"model":"fixture/m", "provider":{"fixture":{"npm":"@ai-sdk/openai", "options":{"baseURL":base, "apiKey":"fixture"}, "models":{"m":{"limit":{"context":65536,"output":4096}}}}}, "permission":{"bash":"ask"}, "agent":{"title":{"disable":true}}}).to_string()).unwrap();
    let (app, guard, _) = oc_adapters::application::spawn_with_env(
        project.path(),
        data.path(),
        BTreeMap::from([
            ("HOME".into(), home.path().display().to_string()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]),
    )
    .await
    .unwrap();
    let session = SessionId::new("owner").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Permission fixture".into())
        .await
        .unwrap();
    app.register_approval_consumer(false).await.unwrap();
    let mut events = app.subscribe();
    app.submit(session.clone(), "invoke tool".into())
        .await
        .unwrap();
    let request = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::PermissionAsked(request) => break request,
                CoreEvent::ToolCallStarted { .. } => panic!("intent before approval"),
                CoreEvent::TurnFinished { .. } => {
                    panic!("turn finished without permission request")
                }
                CoreEvent::TurnFailed { error, .. } => panic!("owner failed: {error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tool_operations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!project.path().join("owner-marker").exists());
    drop(events); // Event consumers may reconnect; queue remains authoritative.
    let mut events = app.subscribe();
    assert_eq!(
        app.pending_approvals().await.unwrap(),
        vec![request.clone()]
    );
    let mut wrong = request.binding.clone();
    wrong.generation += 1;
    assert!(
        app.reply_approval(ApprovalReply {
            id: request.id,
            binding: wrong,
            decision: ApprovalDecision::Once
        })
        .await
        .is_err()
    );
    app.reply_approval(ApprovalReply {
        id: request.id,
        binding: request.binding,
        decision: ApprovalDecision::Once,
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::TurnFinished { .. } => break,
                CoreEvent::TurnFailed { error, .. } => panic!("owner failed: {error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert!(app.pending_approvals().await.unwrap().is_empty());
    assert!(project.path().join("owner-marker").exists());
    app.submit(session.clone(), "invoke second tool".into())
        .await
        .unwrap();
    let pending = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::PermissionAsked(request) => break request,
                CoreEvent::ToolCallStarted { .. } => panic!("second intent before approval"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    app.cancel(session).await.unwrap();
    assert!(app.pending_approvals().await.unwrap().is_empty());
    assert!(
        app.reply_approval(ApprovalReply {
            id: pending.id,
            binding: pending.binding,
            decision: ApprovalDecision::Always
        })
        .await
        .is_err()
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!project.path().join("cancel-owner-marker").exists());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn approval_once_blocks_before_intent_reasks_and_rejects_wrong_digest() {
    let mut permissions = allow_all();
    permissions.insert("bash".into(), Permission::Ask);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation, vec![]);
    let mut events = consumer(&runtime);
    runtime.create_session("once").unwrap();
    let args = serde_json::json!({"argv":["/usr/bin/touch","marker"]});
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("a", "bash", &args)
                + &sse_tool_call("b", "bash", &args)
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let running = runtime.run_turn(params(
        "once",
        "test",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    ));
    let replies = async {
        let first = next_request(&runtime).await;
        assert!(!harness._project.path().join("marker").exists());
        assert!(harness.db.list_tool_ops("once").unwrap().is_empty());
        assert!(
            matches!(&first.preview, ApprovalPreview::Shell { cwd, .. } if cwd == &harness._project.path().display().to_string())
        );
        let mut wrong = first.binding.clone();
        wrong.input_digest.push('x');
        assert!(
            runtime
                .reply_approval(ApprovalReply {
                    id: first.id,
                    binding: wrong,
                    decision: ApprovalDecision::Always
                })
                .is_err()
        );
        assert_eq!(runtime.pending_approvals(), vec![first.clone()]);
        let stale = ApprovalReply {
            id: first.id,
            binding: first.binding.clone(),
            decision: ApprovalDecision::Always,
        };
        answer(&runtime, first, ApprovalDecision::Once);
        assert!(runtime.reply_approval(stale).is_err());
        let second = next_request(&runtime).await;
        assert_eq!(harness.db.list_tool_ops("once").unwrap().len(), 1);
        assert!(harness._project.path().join("marker").exists());
        assert_eq!(second.binding.call, "b");
        answer(&runtime, second, ApprovalDecision::Once);
    };
    let (result, ()) = tokio::join!(running, replies);
    assert_eq!(result.unwrap().calls.len(), 2);
    let mut asked = 0;
    let mut resolved = 0;
    while let Ok(event) = events.try_recv() {
        match event {
            oc_core::core_app::CoreEvent::PermissionAsked(_) => asked += 1,
            oc_core::core_app::CoreEvent::PermissionResolved { .. } => resolved += 1,
            _ => {}
        }
    }
    assert_eq!((asked, resolved), (2, 2));
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn approval_always_commit_failure_keeps_pending_then_survives_runtime_restart() {
    let mut permissions = allow_all();
    permissions.insert("bash".into(), Permission::Ask);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation.clone(), vec![]);
    consumer(&runtime);
    runtime.create_session("always").unwrap();
    let args = serde_json::json!({"argv":["/usr/bin/touch","saved"]});
    let script = vec![
        sse_tool_call("a", "bash", &args) + &sse_tool_call("b", "bash", &args) + &sse_completed(),
        sse_delta("done") + &sse_completed(),
    ];
    let (base, _) = Fake::start(script, Duration::ZERO);
    let running = runtime.run_turn(params(
        "always",
        "test",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    ));
    let replies = async {
        let request = next_request(&runtime).await;
        let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_grant BEFORE INSERT ON permission_grants BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
        assert!(
            runtime
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: request.binding.clone(),
                    decision: ApprovalDecision::Always
                })
                .is_err()
        );
        assert_eq!(runtime.pending_approvals(), vec![request.clone()]);
        assert!(!harness._project.path().join("saved").exists());
        assert!(harness.db.list_tool_ops("always").unwrap().is_empty());
        conn.execute_batch("DROP TRIGGER fail_grant;").unwrap();
        // Every acknowledgement must remain honored beyond the old lexical
        // 1024-row cutoff, both in this runtime and after the SQLite reopen.
        for i in 0..1025 {
            conn.execute(
                "INSERT INTO permission_grants(project,action,pattern) VALUES(?1,?2,?3)",
                rusqlite::params![request.project, request.action, format!("!earlier-{i:04}")],
            )
            .unwrap();
        }
        answer(&runtime, request, ApprovalDecision::Always);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1026
        );
    };
    let (result, ()) = tokio::join!(running, replies);
    assert_eq!(result.unwrap().calls.len(), 2);
    drop(runtime);
    // Reopen SQLite as well as the runtime, not merely an in-memory cache.
    let Harness {
        _project,
        _data,
        db,
        catalog,
    } = harness;
    drop(db);
    let db = Db::open(_data.path()).unwrap();
    let harness = Harness {
        _project,
        _data,
        db,
        catalog,
    };
    let runtime = runtime_of(&harness, generation.clone(), vec![]);
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("c", "bash", &args) + &sse_completed(),
            sse_delta("restart") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    assert_eq!(
        runtime
            .run_turn(params(
                "always",
                "restart",
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
    assert!(runtime.pending_approvals().is_empty());
    let other = tempfile::tempdir().unwrap();
    let isolated = Runtime::new(
        &harness.db,
        "other",
        generation.clone(),
        ProtectedGlobs { patterns: vec![] },
        oc_adapters::files::Files::new(other.path(), harness._data.path()).unwrap(),
        oc_adapters::shell::Shell::new(other.path()).unwrap(),
        BTreeMap::new(),
        oc_adapters::tools::ToolRoots {
            project: other.path().into(),
            data: harness._data.path().into(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .unwrap();
    consumer(&isolated);
    isolated.create_session("isolated").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("d", "bash", &args) + &sse_completed(),
            sse_delta("other") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let running = isolated.run_turn(params(
        "isolated",
        "project",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    ));
    let replies = async {
        let request = next_request(&isolated).await;
        assert!(!other.path().join("saved").exists());
        answer(&isolated, request, ApprovalDecision::Once);
    };
    let (result, ()) = tokio::join!(running, replies);
    assert_eq!(result.unwrap().calls[0].state, "completed");
    let mut denied_generation = generation;
    denied_generation
        .permissions
        .insert("bash".into(), Permission::Deny);
    runtime.reload(denied_generation).await.unwrap();
    consumer(&runtime);
    runtime.register_approval_consumer(true);
    std::fs::remove_file(harness._project.path().join("saved")).unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_tool_call("e", "bash", &args) + &sse_completed(),
            sse_delta("denied") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    assert_eq!(
        runtime
            .run_turn(params(
                "always",
                "deny",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await
            .unwrap()
            .calls[0]
            .state,
        "failed"
    );
    assert!(runtime.pending_approvals().is_empty());
    assert!(!harness._project.path().join("saved").exists());
}

#[tokio::test]
async fn approval_patch_preview_rechecks_real_preimage_and_reject_interrupts_batch() {
    for stale in [false, true] {
        let mut permissions = allow_all();
        permissions.insert("apply_patch".into(), Permission::Ask);
        let (harness, generation) = make_harness(permissions);
        std::fs::write(harness._project.path().join("file"), "old\n").unwrap();
        let runtime = runtime_of(&harness, generation, vec![]);
        consumer(&runtime);
        runtime.create_session("patch").unwrap();
        let patch = "*** Begin Patch\n*** Update File: file\n@@\n-old\n+new\n*** End Patch";
        let (base, _) = Fake::start(
            vec![
                sse_tool_call(
                    "edit",
                    "apply_patch",
                    &serde_json::json!({"patchText":patch}),
                ) + &sse_tool_call(
                    "must-not-run",
                    "bash",
                    &serde_json::json!({"argv":["/usr/bin/touch","later"]}),
                ) + &sse_completed(),
                sse_delta("done") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let running = runtime.run_turn(params(
            "patch",
            "test",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ));
        let replies = async {
            let request = next_request(&runtime).await;
            assert!(harness.db.list_tool_ops("patch").unwrap().is_empty());
            assert_eq!(
                std::fs::read_to_string(harness._project.path().join("file")).unwrap(),
                "old\n"
            );
            let ApprovalPreview::Patch { files, .. } = &request.preview else {
                panic!("real patch preview");
            };
            assert_eq!((files[0].additions, files[0].deletions), (1, 1));
            if stale {
                std::fs::write(harness._project.path().join("file"), "external\n").unwrap();
                answer(&runtime, request, ApprovalDecision::Once);
            } else {
                answer(
                    &runtime,
                    request,
                    ApprovalDecision::Reject { feedback: None },
                );
            }
        };
        let (result, ()) = tokio::join!(running, replies);
        if !stale {
            assert_eq!(result.unwrap().status, TurnStatus::Cancelled);
            assert!(!harness._project.path().join("later").exists());
        } else {
            assert_eq!(result.unwrap().calls[0].state, "failed");
        }
        assert_ne!(
            std::fs::read_to_string(harness._project.path().join("file")).unwrap(),
            "new\n"
        );
    }
}

#[tokio::test]
async fn approval_cancel_drops_waiter_no_started_intent_and_stale_reply_fails() {
    let mut permissions = allow_all();
    permissions.insert("bash".into(), Permission::Ask);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation, vec![]);
    consumer(&runtime);
    runtime.create_session("cancel").unwrap();
    let cancel = AtomicBool::new(false);
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "a",
                "bash",
                &serde_json::json!({"argv":["/usr/bin/touch","never"]}),
            ) + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let running = runtime.run_turn(params(
        "cancel",
        "test",
        &harness,
        provider_of(&base),
        &cancel,
    ));
    let cancelling = async {
        let request = next_request(&runtime).await;
        cancel.store(true, Ordering::Release);
        request
    };
    let (result, request) = tokio::join!(running, cancelling);
    assert_eq!(result.unwrap().status, TurnStatus::Cancelled);
    assert!(runtime.pending_approvals().is_empty());
    assert!(
        runtime
            .reply_approval(ApprovalReply {
                id: request.id,
                binding: request.binding,
                decision: ApprovalDecision::Always
            })
            .is_err()
    );
    assert!(!harness._project.path().join("never").exists());
}
