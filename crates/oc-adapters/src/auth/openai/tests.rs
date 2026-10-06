use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn jwt(value: serde_json::Value) -> String {
    format!(
        "header.{}.signature",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
    )
}

fn oauth(method: Option<&str>, expiry: i64) -> CredentialMaterial {
    CredentialMaterial::OAuth {
        access: "ACCESS_CANARY".into(),
        refresh: Some("REFRESH_CANARY".into()),
        expires_at: Some(expiry),
        method_id: method.map(String::from),
        metadata: Some(OAuthAccountMetadata {
            account_id: "original-account".into(),
        }),
    }
}

fn scope() -> AuthScope {
    AuthScope::admit("openai", OPENAI_BASE_URL).unwrap()
}

async fn issuer(
    status: u16,
    hold: Option<Arc<tokio::sync::Notify>>,
) -> (OpenAiAuth, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut data = Vec::new();
        loop {
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
            if let Some(end) = data.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let size = headers
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                if data.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let text = String::from_utf8(data).unwrap();
        assert!(text.starts_with("POST /oauth/token HTTP/1.1"));
        assert!(text.contains("grant_type=refresh_token"));
        assert!(text.contains("refresh_token=REFRESH_CANARY"));
        assert!(text.contains(CLIENT_ID));
        observed.fetch_add(1, Ordering::SeqCst);
        if let Some(hold) = hold {
            hold.notified().await;
        }
        let body = serde_json::json!({"id_token":jwt(serde_json::json!({})), "access_token":"ROTATED_ACCESS_CANARY", "refresh_token":"ROTATED_REFRESH_CANARY", "expires_in":3600}).to_string();
        socket.write_all(format!("HTTP/1.1 {status} Status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    let mut auth = OpenAiAuth::new().unwrap();
    auth.issuer = url;
    (auth, count, task)
}

async fn dispatched(count: &AtomicUsize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while count.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn auth03_token_schema_claim_priority_defaults_and_redaction() {
    let material = Tokens { id_token: jwt(serde_json::json!({"chatgpt_account_id":"id-direct", "https://api.openai.com/auth":{"chatgpt_account_id":"id-nested"}, "organizations":[{"id":"id-org"}]})), access_token: jwt(serde_json::json!({"chatgpt_account_id":"access-direct"})), refresh_token: "REFRESH_CANARY".into(), expires_in: None }.material(BROWSER, 1000).unwrap();
    assert!(
        matches!(&material, CredentialMaterial::OAuth { expires_at: Some(4600), method_id: Some(id), metadata: Some(metadata), .. } if id==BROWSER && metadata.account_id=="id-direct")
    );
    let access = jwt(
        serde_json::json!({"https://api.openai.com/auth":{"chatgpt_account_id":"access-nested"},"organizations":[{"id":"access-org"}]}),
    );
    let fallback = Tokens {
        id_token: "not-a-jwt".into(),
        access_token: access.clone(),
        refresh_token: "REFRESH_CANARY".into(),
        expires_in: Some(600),
    }
    .material(DEVICE, 1000)
    .unwrap();
    assert!(
        matches!(&fallback, CredentialMaterial::OAuth { metadata: Some(m), .. } if m.account_id=="access-nested")
    );
    assert_eq!(
        claim(&jwt(
            serde_json::json!({"organizations":[{"id":"first"},{"id":"second"}]})
        ))
        .unwrap()
        .account_id,
        "first"
    );
    assert!(claim(&jwt(serde_json::json!({"organizations":"malformed"}))).is_none());
    assert!(!format!("{material:?} {fallback:?}").contains("CANARY"));
    let invalid = Tokens {
        id_token: String::new(),
        access_token: "a\nb".into(),
        refresh_token: "refresh".into(),
        expires_in: Some(-1),
    }
    .material(BROWSER, 1000)
    .unwrap_err();
    assert!(matches!(invalid, AuthError::InvalidTokens));
}

#[tokio::test]
async fn auth03_concurrent_refresh_coalesces_and_rotation_reopens_without_secret_dtos() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let stored = db
        .add_credential(
            scope().namespace(),
            "subscription",
            oauth(Some(BROWSER), 1300),
        )
        .unwrap();
    let shared = db.shared_handle();
    let (auth, count, task) = issuer(200, None).await;
    let (a, b) = tokio::join!(
        auth.resolve_at(
            &db,
            AuthPolicy::Key,
            None,
            || panic!("OAuth must not read env"),
            || 1000
        ),
        auth.resolve_at(
            &shared,
            AuthPolicy::Key,
            None,
            || panic!("OAuth must not read env"),
            || 1000
        )
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(a.source, AuthSource::StoredOAuth);
    assert_eq!(a.key, b.key);
    assert_eq!(a.key.as_deref(), Some("ROTATED_ACCESS_CANARY"));
    assert_eq!(a.oauth.as_ref().unwrap().account_id, "original-account");
    assert_eq!(
        db.credential_accounts(scope().namespace()).unwrap()[0]
            .method_id
            .as_deref(),
        Some(BROWSER)
    );
    assert!(
        !format!(
            "{a:?} {b:?} {:?}",
            db.credential_accounts(scope().namespace()).unwrap()
        )
        .contains("CANARY")
    );
    task.await.unwrap();
    drop(shared);
    drop(db);
    let db = Db::open(root.path()).unwrap();
    let snapshot = db
        .credential_snapshot(scope().namespace())
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.id, stored.id);
    assert_eq!(snapshot.material_revision, 1);
    assert!(!snapshot.refresh_pending);
    assert!(
        matches!(snapshot.material,CredentialMaterial::OAuth{refresh:Some(r),..}if r=="ROTATED_REFRESH_CANARY")
    );
}

#[tokio::test]
async fn auth03_refresh_stale_switch_aba_and_removed_account_never_restore() {
    for remove in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let other = db
            .add_credential(
                scope().namespace(),
                "other",
                CredentialMaterial::Key {
                    key: "OTHER_CANARY".into(),
                },
            )
            .unwrap();
        let account = db
            .add_credential(scope().namespace(), "OAuth", oauth(Some(DEVICE), 1300))
            .unwrap();
        let hold = Arc::new(tokio::sync::Notify::new());
        let (auth, count, task) = issuer(200, Some(hold.clone())).await;
        let shared = db.shared_handle();
        let resolve = tokio::spawn(async move {
            auth.resolve_at(
                &shared,
                AuthPolicy::Key,
                None,
                || panic!("no fallback"),
                || 1000,
            )
            .await
        });
        dispatched(&count).await;
        if remove {
            db.remove_credential(scope().namespace(), &account.id)
                .unwrap();
        } else {
            db.activate_credential(scope().namespace(), &other.id)
                .unwrap();
            db.activate_credential(scope().namespace(), &account.id)
                .unwrap();
        }
        hold.notify_one();
        assert!(matches!(
            resolve.await.unwrap(),
            Err(AuthError::StaleCredential)
        ));
        task.await.unwrap();
        let active = db
            .credential_snapshot(scope().namespace())
            .unwrap()
            .unwrap();
        if remove {
            assert_eq!(active.id, other.id);
        } else {
            assert_eq!(active.id, account.id);
            assert_eq!(active.material_revision, 0);
            assert!(active.refresh_pending);
        }
        assert_eq!(
            db.credential_accounts(scope().namespace()).unwrap().len(),
            if remove { 1 } else { 2 }
        );
    }
}

#[tokio::test]
async fn auth03_unknown_failed_and_cancelled_refresh_are_explicit_and_not_replayed() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let auth = OpenAiAuth::new().unwrap();
    let legacy = db
        .add_credential(scope().namespace(), "legacy", oauth(None, 1300))
        .unwrap();
    assert!(matches!(
        auth.resolve_at(
            &db,
            AuthPolicy::Key,
            Some("CONFIG_CANARY"),
            || panic!("unknown method must not read env"),
            || 1000
        )
        .await,
        Err(AuthError::UnsupportedMethod)
    ));
    db.remove_credential(scope().namespace(), &legacy.id)
        .unwrap();
    let account = db
        .add_credential(scope().namespace(), "known", oauth(Some(BROWSER), 1300))
        .unwrap();
    let (auth, count, task) = issuer(401, None).await;
    assert!(matches!(
        auth.resolve_at(
            &db,
            AuthPolicy::Key,
            None,
            || panic!("no fallback"),
            || 1000
        )
        .await,
        Err(AuthError::Remote)
    ));
    task.await.unwrap();
    assert!(matches!(
        auth.resolve_at(
            &db,
            AuthPolicy::Key,
            Some("CONFIG_CANARY"),
            || panic!("no fallback"),
            || 1000
        )
        .await,
        Err(AuthError::Reauthenticate)
    ));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    db.remove_credential(scope().namespace(), &account.id)
        .unwrap();
    db.add_credential(scope().namespace(), "cancelled", oauth(Some(DEVICE), 1300))
        .unwrap();
    let hold = Arc::new(tokio::sync::Notify::new());
    let (auth, count, task) = issuer(200, Some(hold)).await;
    let shared = db.shared_handle();
    let resolve = tokio::spawn(async move {
        auth.resolve_at(
            &shared,
            AuthPolicy::Key,
            None,
            || panic!("no fallback"),
            || 1000,
        )
        .await
    });
    dispatched(&count).await;
    resolve.abort();
    assert!(resolve.await.unwrap_err().is_cancelled());
    task.abort();
    let _ = task.await;
    drop(db);
    let db = Db::open(root.path()).unwrap();
    assert!(matches!(
        OpenAiAuth::new()
            .unwrap()
            .resolve_at(
                &db,
                AuthPolicy::Key,
                None,
                || panic!("no restart replay"),
                || 1000
            )
            .await,
        Err(AuthError::Reauthenticate)
    ));
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn auth05_key_priority_capture_and_key_oauth_scope_separation() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let auth = OpenAiAuth::new().unwrap();
    let env = auth
        .resolve_at(
            &db,
            AuthPolicy::Key,
            Some("CONFIG_CANARY"),
            || Some("ENV_CANARY".into()),
            || 1000,
        )
        .await
        .unwrap();
    assert_eq!(env.source, AuthSource::OpenAiEnvironment);
    let configured = auth
        .resolve_at(
            &db,
            AuthPolicy::Key,
            Some("CONFIG_CANARY"),
            || None,
            || 1000,
        )
        .await
        .unwrap();
    assert_eq!(configured.source, AuthSource::Configured);
    assert_ne!(env.namespace, configured.namespace);
    assert!(!env.namespace.contains("ENV_CANARY"));
    let stored = db
        .add_credential(
            scope().namespace(),
            "key",
            CredentialMaterial::Key {
                key: "STORED_CANARY".into(),
            },
        )
        .unwrap();
    let key = auth
        .resolve_at(
            &db,
            AuthPolicy::Key,
            Some("CONFIG_CANARY"),
            || panic!("stored wins"),
            || 1000,
        )
        .await
        .unwrap();
    assert_eq!(key.key.as_deref(), Some("STORED_CANARY"));
    db.add_credential(scope().namespace(), "OAuth", oauth(Some(BROWSER), 1301))
        .unwrap();
    let oauth = auth
        .resolve_at(&db, AuthPolicy::Key, None, || panic!("OAuth wins"), || 1000)
        .await
        .unwrap();
    assert_eq!(oauth.source, AuthSource::StoredOAuth);
    assert_ne!(oauth.namespace, key.namespace);
    assert_eq!(key.key.as_deref(), Some("STORED_CANARY")); // issued capture is unchanged
    db.activate_credential(scope().namespace(), &stored.id)
        .unwrap();
    assert!(matches!(
        auth.resolve_at(
            &db,
            AuthPolicy::OAuth,
            None,
            || panic!("explicit OAuth cannot use key"),
            || 1000
        )
        .await,
        Err(AuthError::Reauthenticate)
    ));
}
