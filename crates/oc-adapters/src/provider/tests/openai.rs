use super::*;
use crate::provider::{RequestOverlay, WireBinding, context::RequestContext};

#[tokio::test]
async fn auth04_profile_headers_cannot_rebind_native_account_or_actor_on_http() {
    let server = TestServer::spawn(Arc::new(|_| Action {
        status: "200 OK",
        headers: vec![("Content-Type", "text/event-stream".into())],
        chunks: vec![(b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n".to_vec(), 0)],
        abort_after: None,
    })).await;
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let db = crate::storage::Db::open(&root.path().join("data")).unwrap();
    db.create_session("captured-session").unwrap();
    let overlay = RequestOverlay {
        headers: BTreeMap::from([
            ("originator".into(), "INJECTED_ORIGIN".into()),
            ("x-codex-beta-features".into(), "INJECTED_BETA".into()),
            ("chatgpt-account-id".into(), "INJECTED_ACCOUNT".into()),
            ("session-id".into(), "INJECTED_SESSION".into()),
            ("x-custom-profile".into(), "kept".into()),
        ]),
        body: Default::default(),
    };
    overlay.validate().unwrap();
    for subscription in [true, false] {
        let config = ResponsesConfig {
            base_url: server.base.clone(),
            api_key: "CAPTURED_ACCESS_CANARY".into(),
            timeout: None,
            chunk_timeout_ms: 1000,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: BTreeMap::new(),
            set_cache_key: false,
            wire: WireBinding {
                auth_policy: if subscription {
                    crate::auth::AuthPolicy::OAuth
                } else {
                    crate::auth::AuthPolicy::Key
                },
                endpoint: Some(
                    crate::endpoint::EndpointBinding::admit(&server.base, true, "fixture").unwrap(),
                ),
                openai: Some(crate::auth::OpenAiBinding {
                    scope: "fixture-account".into(),
                    subscription,
                    account: Some("CAPTURED_ACCOUNT_CANARY".into()),
                }),
                ..WireBinding::default()
            },
        }
        .with_context(RequestContext::capture(&db, &project, "captured-session").unwrap());
        crate::provider::stream_input_overlaid(
            &config,
            &overlay,
            "gpt-5.5",
            None,
            &[],
            &[],
            64,
            &AtomicBool::new(false),
            &mut |_| {},
            &mut || async { Ok(()) },
        )
        .await
        .unwrap();
        let seen = server.seen.lock().unwrap().last().unwrap().clone();
        assert_eq!(seen.auth.as_deref(), Some("Bearer CAPTURED_ACCESS_CANARY"));
        assert_eq!(seen.headers.get("x-custom-profile").unwrap(), "kept");
        if subscription {
            assert_eq!(seen.headers.get("session-id").unwrap(), "captured-session");
            assert_eq!(
                seen.headers.get("chatgpt-account-id").unwrap(),
                "CAPTURED_ACCOUNT_CANARY"
            );
            assert_eq!(seen.headers.get("originator").unwrap(), "opencode");
            assert_eq!(
                seen.headers.get("x-codex-beta-features").unwrap(),
                "remote_compaction_v2"
            );
        } else {
            for name in [
                "session-id",
                "chatgpt-account-id",
                "originator",
                "x-codex-beta-features",
            ] {
                assert!(!seen.headers.contains_key(name));
            }
        }
        assert!(
            !seen
                .headers
                .values()
                .any(|value| value.starts_with("INJECTED"))
        );
    }
    assert_eq!(server.attempts.load(Ordering::Relaxed), 2);
    server.handle.abort();
}
