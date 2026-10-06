use super::*;
use crate::cli::{Args, AuthLogin, Command};
use clap::Parser;

#[tokio::test]
async fn auth05_cli_preflight_requires_explicit_safe_inputs_before_storage() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("absent-data");
    for (target, method, answers, expected) in [
        (None, None, vec![], "explicit target"),
        (Some("openai"), None, vec![], "explicit method"),
        (Some("openai"), Some("key"), vec![], "TTYs"),
        (Some("foreign"), Some("key"), vec![], "built-in openai"),
        (Some("openai"), Some("unknown"), vec![], "unknown method"),
        (
            Some("openai"),
            Some("key"),
            vec!["apiKey=SECRET_CANARY"],
            "non-secret label",
        ),
        (
            Some("openai"),
            Some("key"),
            vec!["label="],
            "invalid account label",
        ),
        (
            Some("openai"),
            Some("key"),
            vec!["label=a", "label=b"],
            "invalid account label",
        ),
    ] {
        let action = AuthCommand::Login(AuthLogin {
            target: target.map(str::to_owned),
            method: method.map(str::to_owned),
            answers: answers.into_iter().map(str::to_owned).collect(),
        });
        let error = execute(action, &data, false).await.unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("CANARY"));
        assert!(!data.exists());
    }
    let args = Args::try_parse_from([
        "oc",
        "auth",
        "login",
        "openai",
        "--method",
        "key",
        "--answer",
        "apiKey=SECRET_CANARY",
    ])
    .unwrap();
    assert!(!format!("{args:?}").contains("CANARY"));
    assert!(
        Args::try_parse_from([
            "oc",
            "auth",
            "login",
            "openai",
            "--api-key",
            "SECRET_CANARY"
        ])
        .is_err()
    );
    assert!(matches!(
        Args::try_parse_from(["oc", "auth", "list", "--format", "json"])
            .unwrap()
            .command,
        Some(Command::Auth {
            action: AuthCommand::List {
                format: AuthFormat::Json
            }
        })
    ));
}

#[test]
fn auth05_cli_metadata_and_exact_account_selection_share_scoped_store() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let scope = scope().unwrap();
    let first = db
        .add_credential(
            scope.namespace(),
            "duplicate",
            CredentialMaterial::Key {
                key: "KEY_CANARY".into(),
            },
        )
        .unwrap();
    let second = db
        .add_credential(
            scope.namespace(),
            "duplicate",
            CredentialMaterial::OAuth {
                access: "ACCESS_CANARY".into(),
                refresh: Some("REFRESH_CANARY".into()),
                expires_at: Some(9999999999),
                method_id: Some("chatgpt-browser".into()),
                metadata: Some(oc_adapters::storage::OAuthAccountMetadata {
                    account_id: "ROUTING_CANARY".into(),
                }),
            },
        )
        .unwrap();
    let foreign = AuthScope::admit("openai", "https://foreign.invalid/v1").unwrap();
    db.add_credential(
        foreign.namespace(),
        "FOREIGN_CANARY",
        CredentialMaterial::Key {
            key: "FOREIGN_KEY_CANARY".into(),
        },
    )
    .unwrap();
    let rows = accounts(&db, &scope).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(account(&rows, Some(&first.id), false).unwrap().id, first.id);
    assert_eq!(
        account(&rows, None, false).unwrap_err(),
        "explicit local account ID required without a terminal"
    );
    assert_eq!(
        account(&rows, Some("duplicate"), false).unwrap_err(),
        "account label is ambiguous; use its local account ID"
    );
    assert_eq!(
        account(&rows, Some("missing"), false).unwrap_err(),
        "OpenAI account not found"
    );
    for format in [AuthFormat::Default, AuthFormat::Json] {
        let mut bytes = Vec::new();
        list_to(&mut bytes, &rows, format).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("CANARY"));
        if format == AuthFormat::Json {
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(
                parsed
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|r| r["provider"] == "openai")
            );
            assert!(
                parsed
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["methodID"] == "chatgpt-browser" && r["active"] == true)
            );
            assert!(
                !parsed[0]
                    .as_object()
                    .unwrap()
                    .contains_key("provider_namespace")
            );
        }
    }
    db.activate_credential(scope.namespace(), &first.id)
        .unwrap();
    assert!(
        accounts(&db, &scope)
            .unwrap()
            .iter()
            .find(|r| r.id == first.id)
            .unwrap()
            .active
    );
    db.remove_credential(scope.namespace(), &first.id).unwrap();
    assert!(
        accounts(&db, &scope)
            .unwrap()
            .iter()
            .find(|r| r.id == second.id)
            .unwrap()
            .active
    );
}

#[test]
fn auth05_cli_writer_failure_is_not_an_acknowledgement() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("PRIVATE_CANARY"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for format in [AuthFormat::Default, AuthFormat::Json] {
        assert_eq!(
            list_to(&mut Broken, &[], format).unwrap_err(),
            "auth output unavailable"
        );
    }
}
