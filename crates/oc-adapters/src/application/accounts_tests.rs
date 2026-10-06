use super::*;
use oc_core::domain::SessionId;
use oc_core::queries::{AccountAction, KeyInput, ProviderStatus, SessionProbe};

#[tokio::test]
async fn go05_account_ack_lifecycle_rebinds_without_selection_or_secret_projection() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.set_pref(crate::models_dev::CACHE_KEY, &serde_json::json!({
            "source":crate::models_dev::SOURCE,
            "fetched_at_ms":composition::go_catalog::now_ms(),
            "record":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{
                "m":{"id":"m","name":"Available","tool_call":true,"limit":{"context":10000,"output":1024}}
            }}
        }).to_string()).unwrap();
    }
    let provider = crate::models_dev::PROVIDER.to_owned();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert!(
        app.provider_accounts(provider.clone(), None)
            .await
            .unwrap()
            .accounts
            .is_empty()
    );
    let first = app
        .provider_accounts(
            provider.clone(),
            Some(AccountAction::AddKey {
                label: "First".into(),
                key: KeyInput::new("ACCOUNT_KEY_CANARY_FIRST".into()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(first.accounts.len(), 1);
    assert!(first.accounts[0].active);
    assert_eq!(first.effective, oc_core::queries::AccountAuthSource::Stored);
    assert!(!format!("{first:?}").contains("ACCOUNT_KEY_CANARY"));
    assert!(app.catalog().await.unwrap().selected_model().is_none());
    let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_account BEFORE INSERT ON credential_accounts BEGIN SELECT RAISE(ABORT,'PRIVATE_FAILURE_CANARY'); END;").unwrap();
    let failure = app
        .provider_accounts(
            provider.clone(),
            Some(AccountAction::AddKey {
                label: "Rejected".into(),
                key: KeyInput::new("FAILED_KEY_CANARY".into()),
            }),
        )
        .await
        .unwrap_err();
    assert!(!format!("{failure:?}").contains("CANARY"));
    assert_eq!(
        app.provider_accounts(provider.clone(), None).await.unwrap(),
        first
    );
    conn.execute_batch("DROP TRIGGER reject_account").unwrap();
    drop(conn);
    assert!(
        app.submit(SessionId("never-created".into()), "unchosen".into())
            .await
            .is_err()
    );
    assert_eq!(
        app.probe_session(SessionId("never-created".into()))
            .await
            .unwrap(),
        SessionProbe::Absent
    );
    app.home_selection(oc_core::queries::SessionSelectionAction::Model("m".into()))
        .await
        .unwrap();
    assert_eq!(
        app.home_selection(oc_core::queries::SessionSelectionAction::Current)
            .await
            .unwrap()
            .chrome
            .provider
            .unwrap()
            .status,
        ProviderStatus::Ready
    );
    let second = app
        .provider_accounts(
            provider.clone(),
            Some(AccountAction::AddKey {
                label: "Second".into(),
                key: KeyInput::new("ACCOUNT_KEY_CANARY_SECOND".into()),
            }),
        )
        .await
        .unwrap();
    let second_id = second
        .accounts
        .iter()
        .find(|a| a.active)
        .unwrap()
        .id
        .clone();
    app.provider_accounts(
        provider.clone(),
        Some(AccountAction::Rename {
            id: second_id.clone(),
            label: "Renamed".into(),
        }),
    )
    .await
    .unwrap();
    assert!(
        app.provider_accounts(
            provider.clone(),
            Some(AccountAction::Remove {
                id: second_id.clone(),
                confirmed: false
            })
        )
        .await
        .is_err()
    );
    assert_eq!(
        app.provider_accounts(provider.clone(), None)
            .await
            .unwrap()
            .accounts
            .len(),
        2
    );
    app.provider_accounts(
        provider.clone(),
        Some(AccountAction::Activate {
            id: first.accounts[0].id.clone(),
        }),
    )
    .await
    .unwrap();
    let remaining = app
        .provider_accounts(
            provider.clone(),
            Some(AccountAction::Remove {
                id: first.accounts[0].id.clone(),
                confirmed: true,
            }),
        )
        .await
        .unwrap();
    assert_eq!(remaining.accounts[0].id, second_id);
    assert_eq!(remaining.accounts[0].label, "Renamed");
    assert!(remaining.accounts[0].active);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        app.provider_accounts(provider.clone(), None).await.unwrap(),
        remaining
    );
    assert_eq!(
        app.home_selection(oc_core::queries::SessionSelectionAction::Current)
            .await
            .unwrap()
            .chrome
            .provider
            .unwrap()
            .status,
        ProviderStatus::Ready
    );
    app.provider_accounts(
        provider.clone(),
        Some(AccountAction::Remove {
            id: second_id,
            confirmed: true,
        }),
    )
    .await
    .unwrap();
    // Removing the last stored key must not reuse the previously resolved key.
    assert_ne!(
        app.home_selection(oc_core::queries::SessionSelectionAction::Current)
            .await
            .unwrap()
            .chrome
            .provider
            .unwrap()
            .status,
        ProviderStatus::Ready
    );
    assert!(
        app.submit(SessionId("no-stale-key".into()), "must refuse".into())
            .await
            .is_err()
    );
    assert_eq!(
        app.probe_session(SessionId("no-stale-key".into()))
            .await
            .unwrap(),
        SessionProbe::Absent
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn go05_scoped_account_commands_reject_foreign_ids_and_unconfirmed_removal() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model":"fixture/m","provider":{"fixture":{"options":{"baseURL":"https://example.com/prefix"},"models":{"m":{}}}}
    }).to_string()).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let action = AccountAction::AddKey {
        label: "Scoped".into(),
        key: KeyInput::new("SCOPED_KEY_CANARY".into()),
    };
    assert!(!format!("{action:?}").contains("SCOPED_KEY_CANARY"));
    let added = app
        .provider_accounts("fixture".into(), Some(action))
        .await
        .unwrap();
    assert_eq!(
        app.catalog().await.unwrap().chrome.provider.unwrap().status,
        ProviderStatus::Ready
    );
    assert!(app.provider_accounts("unknown".into(), None).await.is_err());
    assert!(
        app.provider_accounts(
            crate::models_dev::PROVIDER.into(),
            Some(AccountAction::Activate {
                id: added.accounts[0].id.clone()
            })
        )
        .await
        .is_err()
    );
    assert_eq!(
        app.provider_accounts("fixture".into(), None).await.unwrap(),
        added
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
