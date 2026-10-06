use super::*;
use oc_core::queries::{
    AuthAction, AuthAttempt, AuthAttemptFailure, AuthAttemptState, AuthMethod, OAuthMethod,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn presented(app: &CoreApp, id: &str) -> AuthAttempt {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let view = app
                .authenticate("openai".into(), AuthAction::Status { attempt: id.into() })
                .await
                .unwrap();
            assert_eq!(view.state, AuthAttemptState::Pending);
            if view.url.is_some() {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

fn redirect(view: &AuthAttempt) -> reqwest::Url {
    let url = reqwest::Url::parse(view.url.as_ref().unwrap()).unwrap();
    reqwest::Url::parse(
        &url.query_pairs()
            .find(|(key, _)| key == "redirect_uri")
            .unwrap()
            .1,
    )
    .unwrap()
}

#[tokio::test]
async fn auth01_application_owns_attempts_across_locations_and_projects_method_metadata() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    let data = root.path().join("data");
    for path in [&a, &b] {
        std::fs::create_dir(path).unwrap();
    }
    std::fs::write(
        a.join("opencode.json"),
        r#"{"disabled_providers":["opencode-go"]}"#,
    )
    .unwrap();
    std::fs::write(b.join("opencode.json"), r#"{"disabled_providers":["opencode-go"],"provider":{"openai":{"options":{"baseURL":"https://foreign.invalid/v1"},"models":{"m":{}}}}}"#).unwrap();
    let scope = crate::auth::AuthScope::admit("openai", crate::auth::OPENAI_BASE_URL).unwrap();
    let id = {
        let db = Db::open(&data).unwrap();
        db.add_credential(
            scope.namespace(),
            "subscription",
            crate::storage::CredentialMaterial::OAuth {
                access: "APPLICATION_ACCESS_CANARY".into(),
                refresh: Some("APPLICATION_REFRESH_CANARY".into()),
                expires_at: Some(i64::MAX),
                method_id: Some(OAuthMethod::Browser.id().into()),
                metadata: Some(crate::storage::OAuthAccountMetadata {
                    account_id: "APPLICATION_ROUTING_CANARY".into(),
                }),
            },
        )
        .unwrap()
        .id
    };
    let (app, guard, _) = spawn_with_env(&a, &data, BTreeMap::new()).await.unwrap();
    assert!(app.list_sessions().await.unwrap().is_empty());
    assert_eq!(
        app.auth_methods("openai".into()).await.unwrap(),
        vec![
            AuthMethod::OAuth(OAuthMethod::Browser),
            AuthMethod::OAuth(OAuthMethod::Device),
            AuthMethod::Key
        ]
    );
    assert!(app.auth_methods("unknown".into()).await.is_err());
    let accounts = app.provider_accounts("openai".into(), None).await.unwrap();
    assert_eq!(accounts.accounts[0].id, id);
    assert_eq!(
        accounts.accounts[0].method_id.as_deref(),
        Some(OAuthMethod::Browser.id())
    );
    for canary in [
        "APPLICATION_ACCESS_CANARY",
        "APPLICATION_REFRESH_CANARY",
        "APPLICATION_ROUTING_CANARY",
    ] {
        assert!(!format!("{accounts:?}").contains(canary));
    }
    // This slice exposes durable method metadata, not yet OAuth wire execution.
    assert_eq!(
        accounts.effective,
        oc_core::queries::AccountAuthSource::UnsupportedOAuth
    );
    let first = app
        .authenticate(
            "openai".into(),
            AuthAction::Begin {
                method: OAuthMethod::Browser,
                label: "new subscription".into(),
            },
        )
        .await
        .unwrap();
    let first = presented(&app, &first.id).await;
    let address = redirect(&first);
    let changed = app
        .switch_location_home(b.display().to_string())
        .await
        .unwrap();
    assert_eq!(
        changed.location,
        b.canonicalize().unwrap().display().to_string()
    );
    app.reload_location().await.unwrap();
    assert_eq!(
        app.auth_methods("openai".into()).await.unwrap(),
        vec![AuthMethod::Key]
    );
    assert!(
        app.authenticate(
            "openai".into(),
            AuthAction::Begin {
                method: OAuthMethod::Browser,
                label: "foreign".into()
            }
        )
        .await
        .is_err()
    );
    assert!(
        app.authenticate(
            "foreign".into(),
            AuthAction::Cancel {
                attempt: first.id.clone()
            }
        )
        .await
        .is_err()
    );
    let held = app
        .authenticate(
            "openai".into(),
            AuthAction::Status {
                attempt: first.id.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(held, first); // no Location rebinding or secret Debug projection
    let cancelled = app
        .authenticate(
            "openai".into(),
            AuthAction::Cancel {
                attempt: first.id.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        cancelled.state,
        AuthAttemptState::Failed(AuthAttemptFailure::Cancelled)
    );
    assert!(cancelled.url.is_none());
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", address.port().unwrap()))
            .await
            .is_err()
    );
    app.switch_location_home(a.display().to_string())
        .await
        .unwrap();
    let second = app
        .authenticate(
            "openai".into(),
            AuthAction::Begin {
                method: OAuthMethod::Browser,
                label: "callback failure".into(),
            },
        )
        .await
        .unwrap();
    let second = presented(&app, &second.id).await;
    let address = redirect(&second);
    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", address.port().unwrap()))
        .await
        .unwrap();
    socket.write_all(b"GET /auth/callback?code=CALLBACK_CANARY&state=incorrect HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
    let mut html = Vec::new();
    socket.read_to_end(&mut html).await.unwrap();
    assert!(!String::from_utf8_lossy(&html).contains("CALLBACK_CANARY"));
    let failed = app
        .authenticate("openai".into(), AuthAction::Status { attempt: second.id })
        .await
        .unwrap();
    assert_eq!(
        failed.state,
        AuthAttemptState::Failed(AuthAttemptFailure::StateMismatch)
    );
    let last = app
        .authenticate(
            "openai".into(),
            AuthAction::Begin {
                method: OAuthMethod::Browser,
                label: "shutdown pending".into(),
            },
        )
        .await
        .unwrap();
    let last = presented(&app, &last.id).await;
    let address = redirect(&last);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", address.port().unwrap()))
            .await
            .is_err()
    );
    let db = Db::open(&data).unwrap();
    assert_eq!(db.credential_accounts(scope.namespace()).unwrap().len(), 1);
    assert_eq!(db.credential_accounts(scope.namespace()).unwrap()[0].id, id);
    assert!(db.list_sessions().unwrap().is_empty());
}

#[tokio::test]
async fn auth01_application_serves_auth_commands_during_a_held_native_request() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "disabled_providers":["opencode-go"], "model":"fixture/m",
        "provider":{"fixture":{"options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"synthetic"},"models":{"m":{"limit":{"context":100000,"output":2048}}}}}
    }).to_string()).unwrap();
    let (arrived, arrival) = tokio::sync::oneshot::channel();
    let (release, gate) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        let (boundary, length) = loop {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() <= 1_048_576);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let boundary = i + 4;
                let headers = String::from_utf8_lossy(&bytes[..boundary]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                break (boundary, length);
            }
        };
        while bytes.len() < boundary + length {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
        }
        let body: serde_json::Value =
            serde_json::from_slice(&bytes[boundary..boundary + length]).unwrap();
        assert_eq!(body["model"], "m");
        arrived.send(()).unwrap();
        gate.await.unwrap();
        let body = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"done\"}]}]}}\n\n";
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let mut events = app.subscribe();
    let session = SessionId::new("auth-held-request").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.submit(session.clone(), "hold request".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), arrival)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        app.auth_methods("fixture".into()).await.unwrap(),
        vec![AuthMethod::Key]
    );
    let view = app
        .authenticate(
            "openai".into(),
            AuthAction::Begin {
                method: OAuthMethod::Browser,
                label: "while streaming".into(),
            },
        )
        .await
        .unwrap();
    let view = presented(&app, &view.id).await;
    assert!(
        app.provider_accounts("openai".into(), None)
            .await
            .unwrap()
            .accounts
            .is_empty()
    );
    assert_eq!(
        app.authenticate("openai".into(), AuthAction::Cancel { attempt: view.id })
            .await
            .unwrap()
            .state,
        AuthAttemptState::Failed(AuthAttemptFailure::Cancelled)
    );
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::TurnFinished {
                    session: source, ..
                } if source == session => break,
                CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert!(
        db.read_history_full(&session.0)
            .unwrap()
            .iter()
            .all(|row| !format!("{row:?}").contains("while streaming"))
    );
}

#[tokio::test]
async fn auth01_scripted_core_never_masquerades_as_an_auth_owner() {
    let (app, guard) = CoreApp::spawn(oc_core::core_app::MockProvider::echo());
    assert!(app.auth_methods("openai".into()).await.is_err());
    assert!(
        app.authenticate(
            "openai".into(),
            AuthAction::Begin {
                method: OAuthMethod::Browser,
                label: "native only".into()
            }
        )
        .await
        .is_err()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
