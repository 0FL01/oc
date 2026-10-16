use super::*;
use oc_core::queries::KeyInput;

fn input(key: bool) -> CustomProviderInput {
    CustomProviderInput {
        name: "Local service".into(),
        base_url: "http://127.0.0.1:8999/literal".into(),
        auth: if key {
            CustomProviderAuth::Key(KeyInput::new("KEY_CONN_CANARY".into()))
        } else {
            CustomProviderAuth::Anonymous
        },
    }
}

#[test]
fn conn01_atomic_key_anonymous_reopen_and_tagged_authority() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let connection = db
        .create_custom_provider(&input(true), NativeProtocol::Chat, |_| true)
        .unwrap();
    assert_eq!(connection.id.len(), 39);
    assert!(
        connection
            .id
            .strip_prefix("custom-")
            .unwrap()
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let scope = AuthScope::custom(&connection.id, &connection.base_url).unwrap();
    let account = db
        .credential_accounts(scope.namespace())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(account.label, "default");
    assert!(account.active);
    assert_eq!(db.credential_epoch(scope.namespace()), 1);
    for foreign in [
        AuthScope::admit(&connection.id, &connection.base_url).unwrap(),
        AuthScope::custom(&connection.id, "http://127.0.0.1:8999/other").unwrap(),
    ] {
        assert!(db.active_credential(foreign.namespace()).unwrap().is_none());
    }
    let anonymous = db
        .create_custom_provider(&input(false), NativeProtocol::Messages, |_| true)
        .unwrap();
    let anonymous_scope = AuthScope::custom(&anonymous.id, &anonymous.base_url).unwrap();
    assert!(
        db.credential_accounts(anonymous_scope.namespace())
            .unwrap()
            .is_empty()
    );
    assert!(!format!("{connection:?} {:?}", input(true)).contains("CANARY"));
    drop(db);
    let db = Db::open(root.path()).unwrap();
    assert_eq!(db.custom_providers().unwrap().len(), 2);
    assert!(matches!(
        db.active_credential(scope.namespace()).unwrap(),
        Some(CredentialMaterial::Key { .. })
    ));
    db.conn.lock().unwrap().execute("INSERT INTO credential_accounts(id,provider_namespace,label,tagged_value_json,active,created_at) VALUES('corrupt','unused','not-auth','{}',1,0)",[]).unwrap();
    let read_only = Db::saved_custom_providers(root.path()).unwrap();
    assert_eq!(read_only.len(), 2); // works even while the sole writer owns the root
}

#[test]
fn conn01_creation_rollback_and_literal_endpoint_negatives() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_custom_key BEFORE INSERT ON credential_accounts BEGIN SELECT RAISE(ABORT,'KEY_CONN_CANARY'); END;").unwrap();
    let error = db
        .create_custom_provider(&input(true), NativeProtocol::Responses, |_| true)
        .unwrap_err();
    assert!(!format!("{error:?} {error}").contains("CANARY"));
    assert!(db.custom_providers().unwrap().is_empty());
    assert!(db.credential_epochs.lock().unwrap().is_empty());
    for base in [
        "http://169.254.169.254/meta",
        "http://user:key@localhost/api",
        "http://localhost/api?key=secret",
        "http://localhost/{env:KEY}",
        "http://localhost/api#frag",
        " http://localhost/api",
    ] {
        let mut value = input(false);
        value.base_url = base.into();
        assert!(
            db.create_custom_provider(&value, NativeProtocol::Chat, |_| true)
                .is_err(),
            "{base}"
        );
    }
    let mut lan = input(false);
    lan.base_url = "http://192.168.1.2/literal".into();
    assert!(
        db.create_custom_provider(&lan, NativeProtocol::Chat, |_| true)
            .is_ok()
    );
}

#[test]
fn conn05_binding_revision_fences_cache_and_key_changes_in_same_transaction() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let connection = db
        .create_custom_provider(&input(true), NativeProtocol::Responses, |_| true)
        .unwrap();
    let models = BTreeMap::from([("exact/id".into(), serde_json::json!({"name":"ID only"}))]);
    assert!(
        db.save_custom_catalog(&connection.id, 1, &models, CatalogState::Ready)
            .unwrap()
    );
    let scope = AuthScope::custom(&connection.id, &connection.base_url).unwrap();
    db.add_credential(
        scope.namespace(),
        "new",
        CredentialMaterial::Key {
            key: "NEW_KEY_CANARY".into(),
        },
    )
    .unwrap();
    let changed = db.custom_providers().unwrap().pop().unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(changed.catalog_revision, Some(1));
    assert_eq!(changed.state, CatalogState::Missing);
    assert!(
        !db.save_custom_catalog(&connection.id, 1, &BTreeMap::new(), CatalogState::Ready)
            .unwrap()
    );
    assert!(
        db.save_custom_catalog(
            &connection.id,
            2,
            &BTreeMap::new(),
            CatalogState::Unauthorized
        )
        .unwrap()
    );
    assert_eq!(db.custom_providers().unwrap()[0].models, models);
    assert!(
        db.save_custom_catalog(&connection.id, 2, &BTreeMap::new(), CatalogState::Ready)
            .unwrap()
    );
    assert!(db.custom_providers().unwrap()[0].models.is_empty());
    assert_eq!(db.custom_providers().unwrap()[0].catalog_revision, Some(2));
}
