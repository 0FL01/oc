use super::*;

fn request() -> ApprovalRequest {
    ApprovalRequest {
        id: 0,
        binding: ApprovalBinding {
            session: "root".into(),
            turn: "turn".into(),
            call: "call-mcp".into(),
            operation: "operation".into(),
            input_digest: "digest".into(),
            location: "location".into(),
            generation: 3,
            agent: None,
            agent_digest: None,
        },
        project: "project".into(),
        action: "vis36__echo".into(),
        resources: vec!["*".into()],
        save_patterns: vec!["*".into()],
        preview: ApprovalPreview::Tool,
    }
}

#[tokio::test]
async fn tool_marker_retains_action_binding_resource_and_pattern_size_guards() {
    let queue = Arc::new(ApprovalQueue::default());
    queue.register_consumer(true);
    let (tx, mut rx) = broadcast::channel(8);
    for field in 0..5 {
        let mut large = request();
        match field {
            0 => large.action = "x".repeat(16 * 1024 + 1),
            1 => large.binding.input_digest = "x".repeat(16 * 1024 + 1),
            2 => large.resources = vec!["*".into(); 257],
            3 => large.save_patterns = vec!["*".into(); 257],
            _ => large.resources = vec!["x".repeat(128 * 1024 + 1)],
        }
        assert_eq!(
            queue.wait(large, &tx).await,
            Err("approval request too large")
        );
        assert!(queue.pending().is_empty() && rx.try_recv().is_err());
    }
    assert_eq!(queue.wait(request(), &tx).await, Ok(ApprovalDecision::Once));
    let CoreEvent::PermissionAsked(asked) = rx.recv().await.unwrap() else {
        panic!("asked")
    };
    assert_eq!(asked.action, "vis36__echo");
    assert_eq!(asked.resources, ["*"]);
    assert_eq!(asked.preview, ApprovalPreview::Tool);
}

#[tokio::test]
async fn tool_marker_cannot_resolve_a_changed_binding_or_persist_a_stale_reply() {
    let queue = Arc::new(ApprovalQueue::default());
    queue.register_consumer(false);
    let (tx, mut rx) = broadcast::channel(8);
    let task = tokio::spawn({
        let queue = queue.clone();
        let tx = tx.clone();
        async move { queue.wait(request(), &tx).await }
    });
    let CoreEvent::PermissionAsked(asked) = rx.recv().await.unwrap() else {
        panic!("asked")
    };
    let mut reply = ApprovalReply {
        id: asked.id,
        binding: asked.binding.clone(),
        decision: ApprovalDecision::Always,
    };
    reply.binding.input_digest = "foreign digest".into();
    assert_eq!(
        queue.resolve_with(reply.clone(), &tx, |_| panic!("must not persist")),
        Err("approval binding mismatch")
    );
    assert_eq!(queue.pending().as_slice(), std::slice::from_ref(&asked));
    reply.binding = asked.binding;
    queue
        .resolve_with(reply.clone(), &tx, |pending| {
            assert_eq!(pending.action, "vis36__echo");
            assert_eq!(pending.resources, ["*"]);
            assert_eq!(pending.save_patterns, ["*"]);
            Ok(())
        })
        .unwrap();
    assert_eq!(task.await.unwrap(), Ok(ApprovalDecision::Always));
    assert_eq!(
        queue.resolve_with(reply, &tx, |_| panic!("must not persist")),
        Err("stale approval reply")
    );
}
