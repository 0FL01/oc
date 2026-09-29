use super::*;

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
