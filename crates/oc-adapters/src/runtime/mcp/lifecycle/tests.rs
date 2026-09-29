use super::*;

#[test]
fn completion_warning_union_is_typed_and_exact_binding_scoped() {
    let owner = Arc::new(McpOwner::new("old-location", 7));
    let binding = owner.snapshot().binding;
    let original = stdio_attach_error_at("unavailable", "initialize", StdioError::Transport);
    let changed = stdio_attach_error_at("unavailable", "tools-list", StdioError::Deadline);
    let mut initial = McpGeneration::empty(7, owner.shared.wake.clone());
    initial.degraded = vec![original.clone()];
    let mut latest = McpGeneration::empty(7, owner.shared.wake.clone());
    latest.degraded = vec![original.clone(), changed.clone()];
    owner.shared.publication.write().unwrap().request = Arc::new(latest);
    assert_eq!(
        owner.completion_warnings(&binding, &initial),
        [original.to_string(), changed.to_string()]
    );
    assert_eq!(
        initial.degraded.as_slice(),
        std::slice::from_ref(&original),
        "completion mutated the held lease"
    );
    for mismatch in [
        McpBinding {
            location: "new-location".into(),
            ..binding.clone()
        },
        McpBinding {
            generation: 8,
            ..binding.clone()
        },
        McpBinding {
            instance: binding.instance + 1,
            ..binding.clone()
        },
    ] {
        assert_eq!(
            owner.completion_warnings(&mismatch, &initial),
            [original.to_string()]
        );
    }
    let replacement = Arc::new(McpOwner::new("old-location", 7));
    let mut foreign = McpGeneration::empty(7, replacement.shared.wake.clone());
    foreign.degraded = vec![changed];
    replacement.shared.publication.write().unwrap().request = Arc::new(foreign);
    assert_eq!(
        replacement.completion_warnings(&binding, &initial),
        [original.to_string()],
        "same Location/generation but different owner instance mixed warnings"
    );
    // Diagnostic observation cannot clear or demote an existing fatal result.
    owner.shared.publication.write().unwrap().fatal = Some(RuntimeError::McpShutdown);
    let _ = owner.completion_warnings(&binding, &initial);
    assert!(matches!(
        owner.request_view(),
        Err(RuntimeError::McpShutdown)
    ));
}

#[test]
fn successful_retry_clears_the_exact_sanitized_failure_identity() {
    assert_eq!(safe_server_id("known-peer.example"), "known-peer.example");
    for server in ["peer_日本語", &"long_peer_".repeat(12)] {
        let mut degraded = Vec::new();
        record_degradation(
            &mut degraded,
            stdio_attach_error_at(server, "initialize", StdioError::Spawn),
        );
        assert_eq!(degraded.len(), 1);
        assert!(degraded[0].to_string().len() < 160);
        clear_degradation(&mut degraded, server);
        assert!(degraded.is_empty(), "healthy retry retained stale warning");
    }
    let prefix = "long_peer_".repeat(12);
    for (one, two) in [
        (format!("{prefix}one"), format!("{prefix}two")),
        ("peer_日本語".into(), "peer_中文語".into()),
    ] {
        let mut degraded = Vec::new();
        for server in [&one, &two] {
            record_degradation(
                &mut degraded,
                stdio_attach_error_at(server, "initialize", StdioError::Spawn),
            );
            assert!(safe_server_id(server).len() <= 64);
        }
        assert_eq!(degraded.len(), 2, "one failed neighbor hid another");
        clear_degradation(&mut degraded, &one);
        assert_eq!(degraded.len(), 1);
        assert!(
            matches!(&degraded[0], RuntimeError::McpAttach { server, .. } if server == &safe_server_id(&two))
        );
        clear_degradation(&mut degraded, &two);
        assert!(degraded.is_empty());
    }
}

#[tokio::test]
async fn failed_supervisor_join_remains_fatal_after_its_handle_is_consumed() {
    for panic in [false, true] {
        let owner = McpOwner::new("fixture", 1);
        *owner.task.lock().await = Some(tokio::spawn(async move {
            assert!(!panic, "fixture supervisor interruption");
            Err(RuntimeError::McpShutdown)
        }));
        assert_eq!(owner.stop().await, Err(RuntimeError::McpShutdown));
        assert_eq!(owner.stop().await, Err(RuntimeError::McpShutdown));
        assert!(owner.cleanup_failed());
        assert!(matches!(
            owner.request_view(),
            Err(RuntimeError::McpShutdown)
        ));
    }
}
