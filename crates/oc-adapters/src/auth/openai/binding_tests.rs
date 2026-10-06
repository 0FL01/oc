use super::*;
use crate::provider::{ResponsesConfig, WireBinding, context::RequestContext, request_headers};
use std::collections::BTreeMap;

fn config() -> ResponsesConfig {
    ResponsesConfig {
        base_url: OPENAI_BASE_URL.into(),
        api_key: "CONFIG_KEY_CANARY".into(),
        timeout: None,
        chunk_timeout_ms: 1000,
        connect_timeout: Duration::from_secs(1),
        allow_private: false,
        set_cache_key: false,
        headers: BTreeMap::from([
            ("Originator".into(), "INJECTED_ORIGIN".into()),
            ("ChatGPT-Account-Id".into(), "INJECTED_ACCOUNT".into()),
            ("Session-Id".into(), "INJECTED_SESSION".into()),
            ("x-codex-beta-features".into(), "INJECTED_BETA".into()),
            ("OpenAI-Organization".into(), "test-organization".into()),
            ("OpenAI-Project".into(), "test-project".into()),
        ]),
        wire: WireBinding::default(),
    }
}

#[tokio::test]
async fn auth04_each_preparation_refreshes_only_selected_leaf_and_freezes_issued_capture() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    db.create_session("root").unwrap();
    let scope = AuthScope::admit("openai", OPENAI_BASE_URL).unwrap();
    let material = |access: &str, expiry| CredentialMaterial::OAuth {
        access: access.into(),
        refresh: Some("REFRESH_CANARY".into()),
        expires_at: Some(expiry),
        method_id: Some(BROWSER.into()),
        metadata: Some(OAuthAccountMetadata {
            account_id: "ACCOUNT_CANARY".into(),
        }),
    };
    let oauth = db
        .add_credential(
            scope.namespace(),
            "subscription",
            material("ISSUED_ACCESS_CANARY", i64::MAX),
        )
        .unwrap();
    let env = BTreeMap::from([("OPENAI_API_KEY".into(), "ENV_CANARY".into())]);
    let mut original = config();
    original.restore_auth_input();
    scope
        .resolve(&db, AuthPolicy::Key, Some(&original.api_key), false, || {
            None
        })
        .unwrap()
        .apply_to(&mut original);
    let mut alias = original.clone();
    alias.wire.api_model = Some("gpt-5.5".into());
    let mut forbidden = original.clone();
    forbidden.wire.api_model = Some("gpt-5.6".into());
    original.wire.requests.insert(("alias".into(), None), alias);
    original
        .wire
        .requests
        .insert(("forbidden".into(), None), forbidden);
    let original = original.with_context(RequestContext::capture(&db, &project, "root").unwrap());
    let cancel = AtomicBool::new(false);
    let issued = prepare_request(&original, &db, &env, "openai", "alias", None, &cancel)
        .await
        .unwrap();
    let issued_leaf = issued.for_selection("alias", None);
    let authority = issued_leaf.provenance("openai", "alias").unwrap();
    assert_eq!(
        request_headers(issued_leaf).unwrap()["authorization"],
        "Bearer ISSUED_ACCESS_CANARY"
    );
    assert_eq!(request_headers(issued_leaf).unwrap()["session-id"], "root");

    // Time advances through another request boundary, without a generation
    // reload. Two lanes share the existing one refresh flight and rotate once.
    let current = db.credential_snapshot(scope.namespace()).unwrap().unwrap();
    assert!(db.begin_credential_refresh(&current).unwrap());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        db.rotate_credential(&current, material("NEAR_EXPIRY_ACCESS_CANARY", now + 200))
            .unwrap()
    );
    let (auth, count, task) = super::tests::issuer(200, None).await;
    let mut target = issued_leaf.clone();
    target.restore_auth_input();
    let shared = db.shared_handle();
    let (next, peer) = tokio::join!(
        auth.prepare_target(&db, &env, target.clone(), "alias"),
        auth.prepare_target(&shared, &env, target, "alias"),
    );
    let next = install_prepared(&issued, next.unwrap(), "alias", None);
    let next_leaf = next.for_selection("alias", None);
    assert_eq!(
        request_headers(next_leaf).unwrap()["authorization"],
        "Bearer ROTATED_ACCESS_CANARY"
    );
    assert_eq!(peer.unwrap().api_key, "ROTATED_ACCESS_CANARY");
    assert_eq!(next_leaf.provenance("openai", "alias").unwrap(), authority);
    assert_eq!(issued_leaf.api_key, "ISSUED_ACCESS_CANARY");
    assert_eq!(
        next.for_selection("forbidden", None).api_key,
        "ISSUED_ACCESS_CANARY"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    task.await.unwrap();
    let mut selection = crate::models::Selection {
        id: "alias".into(),
        variant: None,
        entry: serde_json::json!({"modelID":"gpt-5.5","limit":{"context":2_000_000,"input":1_900_000,"output":20_000},"cost":{"input":1}}),
    };
    crate::composition::openai_catalog::prepare_selection(&mut selection, &next);
    assert_eq!(
        selection.entry["limit"],
        serde_json::json!({"context":400_000,"input":272_000,"output":20_000})
    );
    assert_eq!(selection.entry["cost"], serde_json::json!([]));

    db.add_credential(
        scope.namespace(),
        "key",
        CredentialMaterial::Key {
            key: "LATER_KEY_CANARY".into(),
        },
    )
    .unwrap();
    let key = prepare_request(&issued, &db, &env, "openai", "alias", None, &cancel)
        .await
        .unwrap();
    let key_leaf = key.for_selection("alias", None);
    assert_eq!(key_leaf.base_url, OPENAI_BASE_URL);
    let headers = request_headers(key_leaf).unwrap();
    assert_eq!(headers["authorization"], "Bearer LATER_KEY_CANARY");
    assert!(!headers.contains_key("session-id"));
    assert!(!headers.contains_key("chatgpt-account-id"));
    assert_ne!(key_leaf.provenance("openai", "alias").unwrap(), authority);
    assert_eq!(
        request_headers(issued_leaf).unwrap()["authorization"],
        "Bearer ISSUED_ACCESS_CANARY"
    );
    db.activate_credential(scope.namespace(), &oauth.id)
        .unwrap();
    assert!(matches!(
        prepare_request(&key, &db, &env, "openai", "forbidden", None, &cancel).await,
        Err(AuthError::ModelUnavailable)
    ));
    let mut pro = key.clone();
    pro.wire
        .requests
        .get_mut(&("alias".into(), None))
        .unwrap()
        .wire
        .settings
        .body
        .insert("reasoning".into(), serde_json::json!({"mode":"pro"}));
    assert!(matches!(
        prepare_request(&pro, &db, &env, "openai", "alias", None, &cancel).await,
        Err(AuthError::ModelUnavailable)
    ));
    assert!(
        !db.credential_snapshot(scope.namespace())
            .unwrap()
            .unwrap()
            .refresh_pending
    );
    assert!(!format!("{issued:?} {next:?} {key:?}").contains("CANARY"));
}

#[tokio::test]
async fn auth04_preparation_keeps_foreign_go_capture_and_refuses_cancelled_admission() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let env = BTreeMap::from([("OPENAI_API_KEY".into(), "ENV_CANARY".into())]);
    let cancel = AtomicBool::new(false);
    let mut foreign = config();
    foreign.base_url = "https://example.invalid/v1".into();
    let captured = prepare_request(&foreign, &db, &env, "openai", "m", None, &cancel)
        .await
        .unwrap();
    assert_eq!(captured, foreign);
    let other = config();
    assert_eq!(
        prepare_request(&other, &db, &env, "other", "m", None, &cancel)
            .await
            .unwrap(),
        other
    );
    let mut go = config();
    go.base_url = super::super::GO_BASE_URL.into();
    go.wire.go = true;
    assert_eq!(
        prepare_request(&go, &db, &env, "opencode-go", "m", None, &cancel)
            .await
            .unwrap(),
        go
    );
    cancel.store(true, Ordering::Relaxed);
    // Non-native callers retain their existing precancel-to-report lifecycle;
    // this new OpenAI preparation boundary does not intercept that contract.
    assert_eq!(
        prepare_request(&go, &db, &env, "opencode-go", "m", None, &cancel)
            .await
            .unwrap(),
        go
    );
    assert_eq!(
        prepare_request(&foreign, &db, &env, "openai", "m", None, &cancel)
            .await
            .unwrap(),
        foreign
    );
    assert!(matches!(
        prepare_request(&other, &db, &env, "openai", "gpt-5.5", None, &cancel).await,
        Err(AuthError::Cancelled)
    ));
    assert!(
        db.credential_accounts(
            AuthScope::admit("openai", OPENAI_BASE_URL)
                .unwrap()
                .namespace()
        )
        .unwrap()
        .is_empty()
    );
    let unavailable = crate::runtime::RuntimeError::from(AuthError::Reauthenticate);
    let crate::runtime::RuntimeError::ProviderUnavailable(diagnostic) = unavailable else {
        panic!("native reauthentication must remain actionable, not a generic provider error");
    };
    assert_eq!(
        diagnostic.action,
        oc_core::queries::ServiceAction::Reauthenticate
    );
    assert_eq!(
        diagnostic.code,
        oc_core::queries::ServiceCode::MissingCredential
    );
    assert!(!diagnostic.to_string().contains("unsupported"));
    let crate::runtime::RuntimeError::ProviderUnavailable(model) =
        crate::runtime::RuntimeError::from(AuthError::ModelUnavailable)
    else {
        panic!("subscription model refusal must preserve explicit selection");
    };
    assert_eq!(model.action, oc_core::queries::ServiceAction::SelectModel);
}

#[tokio::test]
async fn auth04_admitted_oauth_key_capture_route_headers_and_actor_partition() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    let scope = AuthScope::admit("openai", OPENAI_BASE_URL).unwrap();
    let material = |access: &str, account: &str| CredentialMaterial::OAuth {
        access: access.into(),
        refresh: Some("REFRESH_CANARY".into()),
        expires_at: Some(i64::MAX),
        method_id: Some(BROWSER.into()),
        metadata: Some(OAuthAccountMetadata {
            account_id: account.into(),
        }),
    };
    let a = db
        .add_credential(
            scope.namespace(),
            "a",
            material("ACCESS_A_CANARY", "ACCOUNT_A_CANARY"),
        )
        .unwrap();
    let mut captured = config();
    captured.restore_auth_input();
    scope
        .resolve_request(
            &db,
            AuthPolicy::Key,
            Some("CONFIG_KEY_CANARY"),
            false,
            || panic!("stored OAuth cannot fall back to environment"),
        )
        .await
        .unwrap()
        .apply_to(&mut captured);
    assert!(captured.auth_ready());
    assert_eq!(captured.base_url, CODEX_BASE_URL);
    let root_session = "root";
    let child = "other-session";
    db.create_session(root_session).unwrap();
    db.create_session(child).unwrap();
    let first =
        captured.with_context(RequestContext::capture(&db, &project, root_session).unwrap());
    let followup =
        captured.with_context(RequestContext::capture(&db, &project, root_session).unwrap());
    let nested = captured.with_context(RequestContext::capture(&db, &project, child).unwrap());
    for request in [&first, &followup, &nested] {
        let headers = request_headers(request).unwrap();
        assert_eq!(headers["authorization"], "Bearer ACCESS_A_CANARY");
        assert_eq!(headers["originator"], "opencode");
        assert_eq!(headers["x-codex-beta-features"], "remote_compaction_v2");
        assert_eq!(headers["chatgpt-account-id"], "ACCOUNT_A_CANARY");
        assert!(
            !headers
                .values()
                .any(|v| v.as_bytes().starts_with(b"INJECTED"))
        );
        assert!(!format!("{request:?} {:?}", request.wire).contains("CANARY"));
    }
    assert_eq!(request_headers(&first).unwrap()["session-id"], root_session);
    assert_eq!(request_headers(&nested).unwrap()["session-id"], child);
    let provenance = first.provenance("openai", "gpt-5.5").unwrap();
    assert_eq!(provenance, nested.provenance("openai", "gpt-5.5").unwrap());
    // Refresh changes the secret, not the admitted account/opaque authority.
    let old = db.credential_snapshot(scope.namespace()).unwrap().unwrap();
    assert!(db.begin_credential_refresh(&old).unwrap());
    assert!(
        db.rotate_credential(&old, material("ACCESS_ROTATED_CANARY", "ACCOUNT_A_CANARY"))
            .unwrap()
    );
    let mut later = captured.clone();
    later.restore_auth_input();
    scope
        .resolve_request(&db, AuthPolicy::Key, Some(&later.api_key), false, || None)
        .await
        .unwrap()
        .apply_to(&mut later);
    assert_eq!(later.provenance("openai", "gpt-5.5").unwrap(), provenance);
    assert_eq!(
        request_headers(&first).unwrap()["authorization"],
        "Bearer ACCESS_A_CANARY"
    );
    assert_eq!(
        request_headers(&later).unwrap()["authorization"],
        "Bearer ACCESS_ROTATED_CANARY"
    );
    let b = db
        .add_credential(
            scope.namespace(),
            "b",
            material("ACCESS_B_CANARY", "ACCOUNT_B_CANARY"),
        )
        .unwrap();
    later.restore_auth_input();
    scope
        .resolve_request(&db, AuthPolicy::Key, Some(&later.api_key), false, || None)
        .await
        .unwrap()
        .apply_to(&mut later);
    assert_ne!(later.provenance("openai", "gpt-5.5").unwrap(), provenance);
    let key = db
        .add_credential(
            scope.namespace(),
            "key",
            CredentialMaterial::Key {
                key: "KEY_STORED_CANARY".into(),
            },
        )
        .unwrap();
    later.restore_auth_input();
    scope
        .resolve_request(&db, AuthPolicy::Key, Some(&later.api_key), false, || None)
        .await
        .unwrap()
        .apply_to(&mut later);
    assert_eq!(later.base_url, OPENAI_BASE_URL);
    let headers = request_headers(&later).unwrap();
    assert_eq!(headers["authorization"], "Bearer KEY_STORED_CANARY");
    assert_eq!(headers["openai-organization"], "test-organization");
    for name in [
        "originator",
        "chatgpt-account-id",
        "session-id",
        "x-codex-beta-features",
    ] {
        assert!(!headers.contains_key(name));
    }
    assert_ne!(later.provenance("openai", "gpt-5.5").unwrap(), provenance);
    assert_eq!(db.credential_accounts(scope.namespace()).unwrap().len(), 3);
    assert!(a.id != b.id && b.id != key.id);
    let foreign = AuthScope::admit("openai", "https://foreign.invalid/v1").unwrap();
    let foreign_auth = foreign
        .resolve_request(
            &db,
            AuthPolicy::Key,
            Some("FOREIGN_CONFIG_CANARY"),
            false,
            || panic!("foreign scope must not read own OpenAI environment"),
        )
        .await
        .unwrap();
    assert_eq!(foreign_auth.source, AuthSource::Configured);
    assert!(!foreign_auth.openai);
    let mut generic = config();
    foreign_auth.apply_to(&mut generic);
    let generic_headers = request_headers(&generic).unwrap();
    let generic_headers = generic_headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_bytes()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        generic.provenance("custom", "m").unwrap().auth_scope,
        crate::compaction::fingerprint(&("Key", generic_headers))
    );
    let mut rejected = config();
    rejected.wire.auth_policy = AuthPolicy::OAuth;
    assert!(!rejected.auth_ready());
    assert!(request_headers(&rejected).is_err());
}
