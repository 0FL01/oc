use super::*;

fn workspace(owner: &Runtime<'_>, label: &str, permission: Permission) {
    owner
        .publish_workspace(
            Some(label),
            &format!("{label} instructions"),
            vec![(
                "manual".into(),
                format!("---\nname: manual\ndescription: {label} metadata\n---\n{label} body\n"),
            )],
            BTreeMap::new(),
            Some(label.into()),
            Some("build".into()),
            None,
            BTreeMap::from([("read".into(), permission)]),
            Default::default(),
            Default::default(),
        )
        .unwrap();
}

#[test]
fn hot01_captured_publication_keeps_its_workspace_and_policy() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    workspace(&owner, "G1", Permission::Allow);
    let captured = owner.current.read().unwrap().clone();
    let issuing = owner.primary_lane(&captured);

    workspace(&owner, "G2", Permission::Deny);
    owner
        .reload_dcp(DcpConfig {
            debug: !captured.dcp_config.debug,
            ..captured.dcp_config.clone()
        })
        .unwrap();
    owner
        .publish_dcp_protection(ProtectedSpec {
            protect_tags: !captured.dcp_protected.protect_tags,
            ..captured.dcp_protected.clone()
        })
        .unwrap();
    owner
        .publish_provider_state(crate::composition::ProviderState::new(
            "fixture", "G2", false, false,
        ))
        .unwrap();
    let still_issuing = owner.primary_lane(&captured);
    assert_eq!(still_issuing.fixed_input, issuing.fixed_input);
    assert_eq!(still_issuing.agent_digest, issuing.agent_digest);
    assert_eq!(
        RuntimePolicy::with_rules(&still_issuing.permissions, &still_issuing.permission_rules)
            .effect("read", "file"),
        Permission::Allow,
    );

    let current = owner.current.read().unwrap().clone();
    assert!(!Arc::ptr_eq(&captured, &current));
    assert!(
        captured.workspace.skills.entries["manual"]
            .body
            .contains("G1 body")
    );
    assert!(
        current.workspace.skills.entries["manual"]
            .body
            .contains("G2 body")
    );
    assert_ne!(captured.dcp_config.debug, current.dcp_config.debug);
    assert_ne!(
        captured.dcp_protected.protect_tags,
        current.dcp_protected.protect_tags
    );
    assert!(captured.provider_state.is_none());
    assert!(current.provider_state.is_some());
    let fresh = owner.primary_lane(&current);
    assert_ne!(fresh.fixed_input, issuing.fixed_input);
    assert_eq!(fresh.agent_digest.as_deref(), Some("G2"));
    assert_eq!(
        RuntimePolicy::with_rules(&fresh.permissions, &fresh.permission_rules)
            .effect("read", "file"),
        Permission::Deny,
    );
    assert!(db.list_sessions().unwrap().is_empty());
}

#[tokio::test]
async fn hot01_issued_batch_keeps_output_approval_and_instruction_sources() {
    use oc_core::approval::{ApprovalDecision, ApprovalReply};
    use oc_core::question::{QuestionDecision, QuestionReply};
    use serde_json::json;
    use tokio::io::AsyncWriteExt as _;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("nested")).unwrap();
    std::fs::write(
        project.path().join("nested/note.txt"),
        "G1 first line\nG1 second line\nG1 retained final line\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("nested/AGENTS.md"),
        "G1_NESTED_SOURCE\n",
    )
    .unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    owner.update_execution(|view| {
        view.generation
            .config
            .permissions
            .insert("read".into(), Permission::Ask);
        view.generation
            .config
            .permissions
            .insert("opencode_session_rename".into(), Permission::Allow);
        view.generation.config.permission_rules = crate::permissions::PermissionRules::from_config(
            &json!({"permission":{"read":{"*":"ask","nested/AGENTS.md":"allow"},"opencode_session_rename":"allow","opencode_models":"allow","question":"allow"}}),
        )
        .unwrap();
        view.generation.config.providers.insert("inventory".into(), crate::config::ProviderEntry {
            npm: None, name: None, env: Vec::new(), options: Default::default(),
            models: BTreeMap::from([("G1_METADATA".into(), json!({}))]),
        });
        view.workspace.instruction_roots = vec![crate::instructions::Root {
            path: project.path().into(),
            dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
            origin: crate::instructions::Origin::Project,
            baseline: None,
        }];
    });
    let issued = owner.current.read().unwrap().clone();
    let (events, _events) = tokio::sync::broadcast::channel(16);
    owner.set_approval_events(&events);
    owner.register_approval_consumer(false);
    owner.register_question_consumer();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        timeout: None,
        chunk_timeout_ms: 5000,
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        set_cache_key: false,
        wire: Default::default(),
    };
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([(
            "m".into(),
            json!({"limit":{"context":500000,"output":8000}}),
        )]),
    };
    let (received, held) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = binding_replay_tests::request(&listener).await;
        received.send(()).unwrap();
        released.await.unwrap();
        let calls = [
            json!({"type":"function_call","id":"read","call_id":"read","name":"read","arguments":"{\"path\":\"nested/note.txt\"}","status":"completed"}),
            json!({"type":"function_call","id":"rename","call_id":"rename","name":"opencode_session_rename","arguments":"{\"title\":\"G1 completed batch\"}","status":"completed"}),
            json!({"type":"function_call","id":"models","call_id":"models","name":"opencode_models","arguments":"{\"provider\":\"inventory\"}","status":"completed"}),
            json!({"type":"function_call","id":"question","call_id":"question","name":"question","arguments":json!({"questions":[{"question":"Retain G1?","header":"G1","options":[]}]}).to_string(),"status":"completed"}),
        ];
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":calls}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let (mut socket, request) = binding_replay_tests::request(&listener).await;
        let input = request["input"].to_string();
        assert!(
            input.contains("G1 retained final line"),
            "issued output budget, not the new one: {input}"
        );
        assert!(
            input.contains("G1_NESTED_SOURCE"),
            "issued instruction roots: {input}"
        );
        assert!(
            input.contains("G1_METADATA") && !input.contains("G2_METADATA"),
            "issued model inventory: {input}"
        );
        assert!(
            input.contains("G1 answer"),
            "captured question result: {input}"
        );
        binding_replay_tests::respond(
            &mut socket,
            crate::provider::protocol::Protocol::Responses,
            false,
        )
        .await;
    });
    let cancel = AtomicBool::new(false);
    let turn = owner.run_turn(TurnParams {
        session: "s".into(),
        prompt: "read then rename".into(),
        invocation: None,
        catalog: &catalog,
        model_id: "m".into(),
        variant: None,
        max_output: 1024,
        provider,
        cancel: &cancel,
    });
    let publication = async {
        held.await.unwrap();
        // Exercise the private publication seam, not a new public busy-reload API.
        owner.update_execution(|view| {
            view.generation.id = issued.id + 1;
            view.generation.config.tool_output.max_lines = 1;
            view.generation
                .config
                .providers
                .get_mut("inventory")
                .unwrap()
                .models = BTreeMap::from([("G2_METADATA".into(), json!({}))]);
            view.workspace.instruction_roots.clear();
        });
        release.send(()).unwrap();
        let request = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if let Some(request) = owner.pending_approvals().into_iter().next() {
                    break request;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(request.binding.generation, issued.id);
        let mut wrong = request.binding.clone();
        wrong.generation = owner.generation_id();
        assert!(
            owner
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: wrong,
                    decision: ApprovalDecision::Once
                })
                .is_err()
        );
        owner
            .reply_approval(ApprovalReply {
                id: request.id,
                binding: request.binding.clone(),
                decision: ApprovalDecision::Once,
            })
            .unwrap();
        assert!(
            owner
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: request.binding,
                    decision: ApprovalDecision::Once
                })
                .is_err()
        );
        let request = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if let Some(request) = owner.pending_questions().into_iter().next() {
                    break request;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(request.binding.generation, issued.id);
        let mut reply = QuestionReply {
            id: request.id,
            binding: request.binding,
            decision: QuestionDecision::Answers(vec![vec!["G1 answer".into()]]),
        };
        reply.binding.generation = owner.generation_id();
        assert!(owner.reply_question(reply.clone()).is_err());
        reply.binding.generation = issued.id;
        owner.reply_question(reply.clone()).unwrap();
        assert!(owner.reply_question(reply).is_err());
    };
    let (report, ()) = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(turn, publication)
    })
    .await
    .unwrap();
    let report = report.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls.len(), 4, "{:#?}", report.calls);
    assert!(
        report.calls.iter().all(|call| call.state == "completed"),
        "{:#?}",
        report.calls
    );
    peer.await.unwrap();
    assert_eq!(
        db.session_meta("s").unwrap().title.as_deref(),
        Some("G1 completed batch")
    );
    assert!(
        db.list_tool_ops("s")
            .unwrap()
            .iter()
            .all(|op| op.state == "completed")
    );
    assert_eq!(
        db.instruction_view("s").unwrap().1[0].source.generation,
        issued.id
    );
    owner.shutdown_mcp().await.unwrap();
}

#[test]
fn hot01_issuing_turn_fence_rejects_foreign_settled_and_moved_work() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    db.create_bound_session("other", "work").unwrap();
    db.accept_turn(
        "active",
        "s",
        "task",
        "task",
        &oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        },
    )
    .unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    let mut log = TurnLog::new("active", "m", "fixture");
    log.display = serde_json::json!({"location":"work","move_epoch":0,"owning_operation":"active"});
    assert!(
        owner.check_issued_turn(&log, "s").is_err(),
        "a publication is not an execution lease"
    );
    let lease = owner.begin_active().unwrap();
    owner.update_execution(|view| view.generation.id += 1);
    owner.check_issued_turn(&log, "s").unwrap();
    assert!(owner.check_issued_turn(&log, "other").is_err());
    log.display["move_epoch"] = (-1).into();
    assert!(owner.check_issued_turn(&log, "s").is_err());
    log.display["move_epoch"] = 0.into();
    db.finish_turn("active", "cancelled", None).unwrap();
    assert!(owner.check_issued_turn(&log, "s").is_err());
    drop(lease);
    assert!(db.list_tool_ops("s").unwrap().is_empty());
}
