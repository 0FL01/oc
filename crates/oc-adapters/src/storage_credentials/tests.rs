use super::*;
use crate::auth::{AuthPolicy, AuthScope, AuthSource, GO_BASE_URL};

fn key(value: &str) -> CredentialMaterial {
    CredentialMaterial::Key { key: value.into() }
}

#[test]
fn go01_credentials_atomic_lifecycle_reopen_and_secret_safe_projection() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let first = db
        .add_credential("scope-a", "first", key("KEY_CANARY_A"))
        .unwrap();
    let second = db
        .add_credential("scope-a", "second", key("KEY_CANARY_B"))
        .unwrap();
    let oauth = CredentialMaterial::OAuth {
        access: "OAUTH_ACCESS_CANARY".into(),
        refresh: Some("OAUTH_REFRESH_CANARY".into()),
        expires_at: Some(1234),
        method_id: None,
        metadata: None,
    };
    let third = db
        .add_credential("scope-a", "third", oauth.clone())
        .unwrap();
    let other = db
        .add_credential("scope-b", "other", key("KEY_OTHER_CANARY"))
        .unwrap();
    let summaries = db.credential_accounts("scope-a").unwrap();
    assert_eq!(summaries.iter().filter(|account| account.active).count(), 1);
    assert_eq!(summaries[0], third);
    assert_eq!(
        db.active_credential("scope-a").unwrap(),
        Some(oauth.clone())
    );
    assert!(!format!("{summaries:?} {oauth:?}").contains("CANARY"));
    assert!(matches!(
        db.activate_credential("scope-a", &other.id),
        Err(StorageError::CredentialNotFound)
    ));
    db.activate_credential("scope-a", &first.id).unwrap();
    db.rename_credential("scope-a", &first.id, "renamed")
        .unwrap();
    db.remove_credential("scope-a", &second.id).unwrap(); // inactive removal cannot change first
    assert_eq!(
        db.active_credential("scope-a").unwrap(),
        Some(key("KEY_CANARY_A"))
    );
    db.remove_credential("scope-a", &first.id).unwrap();
    assert_eq!(
        db.active_credential("scope-a").unwrap(),
        Some(oauth.clone())
    );
    drop(db);
    let db = Db::open(root.path()).unwrap();
    assert_eq!(db.active_credential("scope-a").unwrap(), Some(oauth));
    assert_eq!(
        db.credential_accounts("scope-a").unwrap(),
        vec![third.clone()]
    );
    db.remove_credential("scope-a", &third.id).unwrap();
    assert!(db.active_credential("scope-a").unwrap().is_none());
    assert_eq!(db.credential_accounts("scope-b").unwrap(), vec![other]);
    let conn = db.conn.lock().unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM schema_migrations WHERE version=11",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn go01_credential_transactions_rollback_constraints_and_redacted_errors() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let first = db
        .add_credential("scope", "first", key("SECRET_ROLLBACK_CANARY"))
        .unwrap();
    let second = db
        .add_credential("scope", "second", key("OTHER_KEY_CANARY"))
        .unwrap();
    {
        let conn = db.conn.lock().unwrap();
        assert!(
            conn.execute(
                "UPDATE credential_accounts SET active=1 WHERE id=?1",
                [&first.id]
            )
            .is_err()
        );
        conn.execute_batch("CREATE TRIGGER credential_fail BEFORE UPDATE OF active ON credential_accounts WHEN NEW.id != OLD.id OR NEW.active=1 BEGIN SELECT RAISE(ABORT,'SECRET_ROLLBACK_CANARY'); END;").unwrap();
    }
    for result in [
        db.activate_credential("scope", &first.id),
        db.remove_credential("scope", &second.id),
    ] {
        let error = result.unwrap_err();
        assert!(matches!(error, StorageError::CredentialStorage));
        assert!(!format!("{error:?} {error}").contains("CANARY"));
        assert_eq!(
            db.active_credential("scope").unwrap(),
            Some(key("OTHER_KEY_CANARY"))
        );
        assert_eq!(db.credential_accounts("scope").unwrap().len(), 2);
        assert!(db.conn.lock().unwrap().is_autocommit());
    }
    db.conn.lock().unwrap().execute_batch("DROP TRIGGER credential_fail; CREATE TRIGGER credential_insert_fail BEFORE INSERT ON credential_accounts BEGIN SELECT RAISE(ABORT,'SECRET_ROLLBACK_CANARY'); END;").unwrap();
    assert!(matches!(
        db.add_credential("scope", "third", key("THIRD_CANARY")),
        Err(StorageError::CredentialStorage)
    ));
    assert_eq!(
        db.active_credential("scope").unwrap(),
        Some(key("OTHER_KEY_CANARY"))
    );
    assert!(matches!(
        db.add_credential("scope", "bad", key("\n")),
        Err(StorageError::InvalidCredential)
    ));
}

#[test]
fn go01_auth_authority_priority_scope_and_unsupported_oauth() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    for url in [
        "http://opencode.ai/zen/go/v1",
        "https://private.invalid/zen/go/v1",
        "https://opencode.ai/zen/go/v10",
        "https://opencode.ai/zen/go/v1?x=1",
    ] {
        assert!(AuthScope::admit("opencode-go", url).is_err());
    }
    let go = AuthScope::admit("opencode-go", &format!("{GO_BASE_URL}/")).unwrap();
    let env = || Some("GO_ENV_CANARY".into());
    let resolved = go
        .resolve(&db, AuthPolicy::Key, Some("CONFIG_CANARY"), false, env)
        .unwrap();
    assert_eq!(resolved.source, AuthSource::GoEnvironment);
    assert_eq!(resolved.key.as_deref(), Some("GO_ENV_CANARY"));
    assert!(!format!("{resolved:?}").contains("CANARY"));
    db.add_credential(go.namespace(), "Go", key("STORED_CANARY"))
        .unwrap();
    assert_eq!(
        go.resolve(
            &db,
            AuthPolicy::Key,
            Some("CONFIG_CANARY"),
            false,
            || panic!("stored has priority")
        )
        .unwrap()
        .source,
        AuthSource::Stored
    );
    let custom = AuthScope::admit("custom", "https://custom.invalid/prefix").unwrap();
    assert_eq!(
        custom
            .resolve(&db, AuthPolicy::Key, None, false, || panic!(
                "no foreign autoenv"
            ))
            .unwrap()
            .source,
        AuthSource::Missing
    );
    db.add_credential(custom.namespace(), "custom", key("CUSTOM_STORED_CANARY"))
        .unwrap();
    assert_eq!(
        custom
            .resolve(
                &db,
                AuthPolicy::Key,
                Some("CUSTOM_CONFIG_CANARY"),
                false,
                env
            )
            .unwrap()
            .source,
        AuthSource::Configured
    );
    for scope in [
        AuthScope::admit("custom", "https://custom.invalid/new-prefix").unwrap(),
        AuthScope::admit("other", "https://custom.invalid/prefix").unwrap(),
    ] {
        assert_eq!(
            scope
                .resolve(&db, AuthPolicy::Key, None, false, env)
                .unwrap()
                .source,
            AuthSource::Missing
        );
    }
    let oauth = CredentialMaterial::OAuth {
        access: "ACCESS_CANARY".into(),
        refresh: None,
        expires_at: None,
        method_id: None,
        metadata: None,
    };
    db.add_credential(custom.namespace(), "OAuth", oauth)
        .unwrap();
    assert_eq!(
        custom
            .resolve(&db, AuthPolicy::Key, None, false, env)
            .unwrap()
            .source,
        AuthSource::UnsupportedOAuth
    );
    assert_eq!(
        custom
            .resolve(&db, AuthPolicy::None, None, false, || panic!(
                "None does not resolve material"
            ))
            .unwrap()
            .source,
        AuthSource::Anonymous
    );
    assert!(
        custom
            .resolve(&db, AuthPolicy::None, Some("key"), false, env)
            .is_err()
    );
    assert!(
        custom
            .resolve(&db, AuthPolicy::None, None, true, env)
            .is_err()
    );
    let mut provider = crate::provider::ResponsesConfig {
        base_url: "https://custom.invalid/prefix".into(),
        api_key: String::new(),
        timeout: None,
        chunk_timeout_ms: 1000,
        connect_timeout: Duration::from_secs(1),
        allow_private: false,
        headers: Default::default(),
        set_cache_key: false,
        wire: Default::default(),
    };
    assert!(!provider.auth_ready());
    custom
        .resolve(&db, AuthPolicy::None, None, false, env)
        .unwrap()
        .apply_to(&mut provider);
    assert!(provider.auth_ready());
    let headers = crate::provider::request_headers(&provider).unwrap();
    assert!(!headers.contains_key("authorization"));
    assert!(!headers.contains_key("x-api-key"));
    provider
        .headers
        .insert("Authorization".into(), "FOREIGN_CANARY".into());
    assert!(crate::provider::request_headers(&provider).is_err());
    provider.headers.clear();
    custom
        .resolve(&db, AuthPolicy::Key, None, false, env)
        .unwrap()
        .apply_to(&mut provider);
    assert!(!provider.auth_ready());
    assert!(crate::provider::request_headers(&provider).is_err());
}

#[test]
fn go01_sqlite_secret_files_are_private_and_symlinks_refused() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.add_credential("scope", "private", key("PRIVATE_CANARY"))
        .unwrap();
    for name in ["oc.sqlite", "oc.sqlite-wal", "oc.sqlite-shm"] {
        let metadata = fs::metadata(root.path().join(name)).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600, "{name}");
    }
    drop(db);
    let other = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        root.path().join("oc.sqlite"),
        other.path().join("oc.sqlite"),
    )
    .unwrap();
    assert!(Db::open(other.path()).is_err());
}

#[test]
fn auth03_legacy_oauth_additive_upgrade_method_metadata_and_cas_identity() {
    let root = tempfile::tempdir().unwrap();
    {
        let db = Db::open(root.path()).unwrap();
        let conn = db.conn.lock().unwrap();
        conn.execute_batch("ALTER TABLE credential_accounts DROP COLUMN selection_revision;
            ALTER TABLE credential_accounts DROP COLUMN material_revision;
            ALTER TABLE credential_accounts DROP COLUMN refresh_pending;
            DELETE FROM schema_migrations WHERE version=13;
            INSERT INTO credential_accounts(id,provider_namespace,label,tagged_value_json,active,created_at)
            VALUES('legacy','scope','legacy','{\"type\":\"oauth\",\"access\":\"LEGACY_ACCESS_CANARY\",\"refresh\":\"LEGACY_REFRESH_CANARY\",\"expires_at\":1234}',1,1);").unwrap();
    }
    let db = Db::open(root.path()).unwrap();
    let legacy = db.credential_snapshot("scope").unwrap().unwrap();
    assert_eq!(legacy.selection_revision, 0);
    assert_eq!(legacy.material_revision, 0);
    assert!(!legacy.refresh_pending);
    assert!(matches!(
        &legacy.material,
        CredentialMaterial::OAuth {
            method_id: None,
            metadata: None,
            ..
        }
    ));
    assert!(!format!("{legacy:?}").contains("CANARY"));
    let material = CredentialMaterial::OAuth {
        access: "ACCESS_CANARY".into(),
        refresh: Some("REFRESH_CANARY".into()),
        expires_at: Some(4600),
        method_id: Some("chatgpt-browser".into()),
        metadata: Some(OAuthAccountMetadata {
            account_id: "fixture-account".into(),
        }),
    };
    let account = db.add_credential("scope", "new", material.clone()).unwrap();
    assert_eq!(account.method_id.as_deref(), Some("chatgpt-browser"));
    let captured = db.credential_snapshot("scope").unwrap().unwrap();
    assert!(db.begin_credential_refresh(&captured).unwrap());
    db.rename_credential("scope", &account.id, "renamed during refresh")
        .unwrap();
    assert!(db.rotate_credential(&captured, material.clone()).unwrap());
    assert!(!db.rotate_credential(&captured, material.clone()).unwrap());
    let fresh = db.credential_snapshot("scope").unwrap().unwrap();
    assert_eq!(fresh.material_revision, captured.material_revision + 1);
    assert_eq!(fresh.selection_revision, captured.selection_revision);
    assert!(!fresh.refresh_pending);
    db.activate_credential("scope", &account.id).unwrap(); // same active identity is idempotent
    assert_eq!(
        db.credential_snapshot("scope")
            .unwrap()
            .unwrap()
            .selection_revision,
        fresh.selection_revision
    );
    assert!(db.begin_credential_refresh(&fresh).unwrap());
    db.activate_credential("scope", "legacy").unwrap();
    db.activate_credential("scope", &account.id).unwrap();
    assert!(!db.rotate_credential(&fresh, material).unwrap());
    let summaries = db.credential_accounts("scope").unwrap();
    assert_eq!(summaries[0].label, "renamed during refresh");
    assert!(!format!("{summaries:?}").contains("CANARY"));
    drop(db);
    let db = Db::open(root.path()).unwrap();
    let row = db.credential_snapshot("scope").unwrap().unwrap();
    assert!(row.refresh_pending);
    assert_eq!(row.selection_revision, fresh.selection_revision + 2);
    assert_eq!(row.material_revision, fresh.material_revision);
    let conn = db.conn.lock().unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM schema_migrations WHERE version=13",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
