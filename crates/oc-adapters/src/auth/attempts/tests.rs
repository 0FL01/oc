use super::*;
use crate::storage::CredentialMaterial;
use serde_json::{Value, json};

#[derive(Clone)]
struct Request {
    path: String,
    body: String,
    at: Instant,
}
struct Issuer {
    auth: OpenAiAuth,
    requests: Arc<Mutex<Vec<Request>>>,
    worker: JoinHandle<()>,
}
async fn issuer(replies: Vec<(u16, Value)>, hold: Option<Arc<tokio::sync::Notify>>) -> Issuer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut auth = OpenAiAuth::new().unwrap();
    auth.issuer = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let worker = tokio::spawn(async move {
        for (status, body) in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = Vec::new();
            let (end, size) = loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0 && data.len() + n <= 64 * 1024);
                data.extend_from_slice(&buf[..n]);
                if let Some(end) = data.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&data[..end]).unwrap();
                    let size = headers
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    if data.len() >= end + 4 + size {
                        break (end, size);
                    }
                }
            };
            let headers = std::str::from_utf8(&data[..end]).unwrap();
            assert!(headers.starts_with("POST ") && headers.contains("opencode/"));
            let path = headers.split_whitespace().nth(1).unwrap().into();
            observed.lock().unwrap().push(Request {
                path,
                body: String::from_utf8(data[end + 4..end + 4 + size].to_vec()).unwrap(),
                at: Instant::now(),
            });
            if let Some(hold) = &hold {
                hold.notified().await;
            }
            let body = body.to_string();
            let _ = socket.write_all(format!("HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
        }
    });
    Issuer {
        auth,
        requests,
        worker,
    }
}
fn tokens() -> Value {
    let claim = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(json!({"chatgpt_account_id":"subscription-account"}).to_string());
    json!({"id_token":format!("h.{claim}.s"), "access_token":"ACCESS_CANARY", "refresh_token":"REFRESH_CANARY", "expires_in":3600})
}
fn owner(db: &Db, auth: OpenAiAuth) -> OpenAiAttempts {
    let mut owner = OpenAiAttempts::new(db).unwrap();
    owner.auth = Arc::new(auth);
    owner.settings.ports = [0, 0];
    owner.settings.bind_delay = Duration::from_millis(1);
    owner
}
fn namespace() -> String {
    AuthScope::admit("openai", OPENAI_BASE_URL)
        .unwrap()
        .namespace()
        .into()
}
async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
async fn presented(owner: &OpenAiAttempts, id: &str) -> AuthAttempt {
    until(|| owner.status(id).unwrap().url.is_some()).await;
    owner.status(id).unwrap()
}
async fn terminal(owner: &OpenAiAttempts, id: &str) -> AuthAttempt {
    until(|| owner.status(id).unwrap().state != State::Pending).await;
    owner.status(id).unwrap()
}
fn callback(view: &AuthAttempt, code: &str) -> reqwest::Url {
    let authorize = reqwest::Url::parse(view.url.as_ref().unwrap()).unwrap();
    let params: BTreeMap<_, _> = authorize
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut callback = reqwest::Url::parse(&params["redirect_uri"]).unwrap();
    callback
        .query_pairs_mut()
        .extend_pairs([("code", code), ("state", params["state"].as_str())]);
    callback
}

#[tokio::test]
async fn auth01_browser_pkce_owned_callback_durable_ack_and_duplicate_cannot_store_twice() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let server = issuer(vec![(200, tokens())], None).await;
    let owner = owner(&db, server.auth);
    let begun = owner
        .begin(OAuthMethod::Browser, "subscription".into())
        .await
        .unwrap();
    let view = presented(&owner, &begun.id).await;
    assert_eq!(view.method.label(), "ChatGPT Pro/Plus (browser)");
    let authorize = reqwest::Url::parse(view.url.as_ref().unwrap()).unwrap();
    let params: BTreeMap<_, _> = authorize
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(params.len(), 10);
    for (name, value) in [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("scope", "openid profile email offline_access"),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("originator", "opencode"),
    ] {
        assert_eq!(params[name], value);
    }
    assert_eq!(params["state"].len(), 43);
    assert_eq!(params["code_challenge"].len(), 43);
    assert!(!format!("{view:?}").contains(&params["state"]));
    let url = callback(&view, "CODE_CANARY");
    let mut wrong = url.clone();
    wrong.set_path("/wrong");
    let response = reqwest::get(wrong).await.unwrap();
    assert_eq!(response.status().as_u16(), 404);
    assert_eq!(owner.status(&begun.id).unwrap().state, State::Pending);
    let response = reqwest::get(url.clone()).await.unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let html = response.text().await.unwrap();
    assert!(html.contains("Authorization successful") && html.contains("window.close"));
    assert!(!html.contains("CANARY"));
    let complete = terminal(&owner, &begun.id).await;
    assert_eq!(complete.state, State::Complete);
    let accounts = db.credential_accounts(&namespace()).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(
        complete.account_id.as_deref(),
        Some(accounts[0].id.as_str())
    );
    assert_eq!(accounts[0].method_id.as_deref(), Some(BROWSER));
    assert!(reqwest::get(url).await.is_err());
    assert_eq!(db.credential_accounts(&namespace()).unwrap().len(), 1);
    server.worker.await.unwrap();
    let requests = server.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/oauth/token");
    let form = reqwest::Url::parse(&format!("http://localhost/?{}", requests[0].body)).unwrap();
    let fields: BTreeMap<_, _> = form
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(fields["code"], "CODE_CANARY");
    assert_eq!(fields["grant_type"], "authorization_code");
    assert_eq!(fields["redirect_uri"], params["redirect_uri"]);
    assert_eq!(fields["code_verifier"].len(), 43);
    assert_eq!(
        params["code_challenge"],
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(sha2::Sha256::digest(fields["code_verifier"].as_bytes()))
    );
    drop(requests);
    owner.shutdown().await.unwrap();
    drop(owner);
    drop(db);
    let db = Db::open(root.path()).unwrap();
    assert_eq!(db.credential_accounts(&namespace()).unwrap().len(), 1);
}

#[tokio::test]
async fn auth01_invalid_callbacks_owned_cancel_ports_expiry_and_safe_pages() {
    assert_eq!(Settings::default().ports, [1455, 1457]);
    assert_eq!(Settings::default().bind_delay, Duration::from_millis(200));
    assert_eq!(Settings::default().lifetime, Duration::from_secs(600));
    assert_eq!(Settings::default().retention, Duration::from_secs(60));
    assert_eq!(Settings::default().cleanup, Duration::from_secs(30));
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port();
    let mut owner = owner(&db, OpenAiAuth::new().unwrap());
    owner.settings.ports = [port, 0];
    for (query, failure) in [
        ("code=CODE_CANARY&state=wrong", Failure::StateMismatch),
        ("state=wrong", Failure::Callback),
        (
            "error=%3Cscript%3ESECRET_CANARY%3C%2Fscript%3E",
            Failure::Callback,
        ),
    ] {
        let begun = owner
            .begin(OAuthMethod::Browser, "subscription".into())
            .await
            .unwrap();
        let view = presented(&owner, &begun.id).await;
        let mut url = callback(&view, "unused");
        url.set_query(Some(query));
        assert_ne!(url.port().unwrap(), port);
        let response = reqwest::get(url.clone()).await.unwrap();
        assert_eq!(response.status().as_u16(), 400);
        let html = response.text().await.unwrap();
        assert!(!html.contains("CANARY") && !html.contains("<script>SECRET"));
        assert_eq!(
            terminal(&owner, &begun.id).await.state,
            State::Failed(failure)
        );
        assert!(reqwest::get(url).await.is_err());
    }
    // The unrelated service receives no /cancel probe (or any connection).
    assert!(
        tokio::time::timeout(Duration::from_millis(20), occupied.accept())
            .await
            .is_err()
    );
    assert!(page(false, "<script>\"'&").contains("&lt;script&gt;&quot;&#39;&amp;"));
    let a = owner.begin(OAuthMethod::Browser, "a".into()).await.unwrap();
    let a = presented(&owner, &a.id).await;
    let old_callback = callback(&a, "unused");
    let b = owner.begin(OAuthMethod::Browser, "b".into()).await.unwrap();
    let b = presented(&owner, &b.id).await;
    assert_eq!(
        owner.status(&a.id).unwrap().state,
        State::Failed(Failure::Cancelled)
    );
    assert!(reqwest::get(old_callback).await.is_err());
    let cancelled = owner.cancel(&b.id).await.unwrap();
    assert_eq!(cancelled.state, State::Failed(Failure::Cancelled));
    assert!(reqwest::get(callback(&b, "unused")).await.is_err());
    assert!(db.credential_accounts(&namespace()).unwrap().is_empty());
    owner.shutdown().await.unwrap();
    assert!(
        owner
            .begin(OAuthMethod::Browser, "closed".into())
            .await
            .is_err()
    );
    drop(owner);
    let mut owner = OpenAiAttempts::new(&db).unwrap();
    owner.settings.lifetime = Duration::from_millis(30);
    owner.settings.ports = [0, 0];
    owner.settings.retention = Duration::from_millis(20);
    owner.settings.cleanup = Duration::from_millis(5);
    let expiring = owner
        .begin(OAuthMethod::Browser, "expired".into())
        .await
        .unwrap();
    let expiring = presented(&owner, &expiring.id).await;
    let url = callback(&expiring, "unused");
    assert_eq!(terminal(&owner, &expiring.id).await.state, State::Expired);
    assert!(reqwest::get(url).await.is_err());
    until(|| owner.status(&expiring.id).is_err()).await;
    assert!(owner.entries.lock().unwrap().is_empty());
    // Cleanup stops while idle and a subsequent attempt gets a new owned timer.
    let fresh = owner
        .begin(OAuthMethod::Browser, "fresh".into())
        .await
        .unwrap();
    owner.shutdown().await.unwrap();
    assert!(owner.status(&fresh.id).is_err());
}

#[tokio::test]
async fn auth02_device_pending_intervals_exchange_and_durable_reopen() {
    assert_eq!(OAuthMethod::Device.label(), "ChatGPT Pro/Plus (headless)");
    assert_eq!(Settings::default().polling_margin, Duration::from_secs(3));
    for (value, seconds) in [
        (None, 5),
        (Some("0"), 5),
        (Some("-2"), 1),
        (Some("2.5"), 2),
        (Some("  +3seconds"), 3),
    ] {
        assert_eq!(device_interval(value), Duration::from_secs(seconds));
    }
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let server=issuer(vec![(200,json!({"device_auth_id":"DEVICE_ID_CANARY","user_code":"USER_CODE_CANARY","interval":"-1"})),(403,json!({})),(404,json!({})),(200,json!({"authorization_code":"DEVICE_CODE_CANARY","code_verifier":"DEVICE_VERIFIER_CANARY"})),(200,tokens())],None).await;
    let mut owner = owner(&db, server.auth);
    owner.settings.polling_margin = Duration::from_millis(10);
    // Both preferred ports may be occupied: device flow never binds either.
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port();
    owner.settings.ports = [port, port];
    let begun = owner
        .begin(OAuthMethod::Device, "headless".into())
        .await
        .unwrap();
    let view = presented(&owner, &begun.id).await;
    assert!(view.url.as_deref().unwrap().ends_with("/codex/device"));
    assert_eq!(
        view.instructions.as_deref(),
        Some("Enter code: USER_CODE_CANARY")
    );
    assert!(!format!("{view:?}").contains("CANARY"));
    assert_eq!(terminal(&owner, &begun.id).await.state, State::Complete);
    server.worker.await.unwrap();
    let requests = server.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].path, "/api/accounts/deviceauth/usercode");
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body).unwrap(),
        json!({"client_id":CLIENT_ID})
    );
    for index in 1..=3 {
        assert_eq!(requests[index].path, "/api/accounts/deviceauth/token");
        assert_eq!(
            serde_json::from_str::<Value>(&requests[index].body).unwrap(),
            json!({"device_auth_id":"DEVICE_ID_CANARY","user_code":"USER_CODE_CANARY"})
        );
        if index > 1 {
            assert!(
                requests[index].at.duration_since(requests[index - 1].at) >= Duration::from_secs(1)
            );
        }
    }
    assert_eq!(requests[4].path, "/oauth/token");
    assert!(
        requests[4]
            .body
            .contains("code_verifier=DEVICE_VERIFIER_CANARY")
            && requests[4].body.contains("deviceauth%2Fcallback")
    );
    drop(requests);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), occupied.accept())
            .await
            .is_err()
    );
    owner.shutdown().await.unwrap();
    drop(owner);
    drop(db);
    let db = Db::open(root.path()).unwrap();
    let accounts = db.credential_accounts(&namespace()).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].method_id.as_deref(), Some(DEVICE));
}

#[tokio::test]
async fn auth03_late_login_cannot_change_selection_and_cancelled_exchange_is_not_replayed() {
    for cancel in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let hold = Arc::new(tokio::sync::Notify::new());
        let server = issuer(vec![(200, tokens())], Some(hold.clone())).await;
        let owner = owner(&db, server.auth);
        let begun = owner
            .begin(OAuthMethod::Browser, "late".into())
            .await
            .unwrap();
        let view = presented(&owner, &begun.id).await;
        let callback = callback(&view, "CODE_CANARY");
        let get = tokio::spawn(async move { reqwest::get(callback).await });
        until(|| server.requests.lock().unwrap().len() == 1).await;
        if cancel {
            assert_eq!(
                owner.cancel(&begun.id).await.unwrap().state,
                State::Failed(Failure::Cancelled)
            );
        } else {
            // Empty-namespace ABA also fences late activation, across shared Db handles.
            let shared = db.shared_handle();
            let key = shared
                .add_credential(
                    &namespace(),
                    "key",
                    CredentialMaterial::Key {
                        key: "KEY_CANARY".into(),
                    },
                )
                .unwrap();
            shared.remove_credential(&namespace(), &key.id).unwrap();
        }
        hold.notify_one();
        server.worker.await.unwrap();
        let _ = get.await.unwrap();
        assert_eq!(
            terminal(&owner, &begun.id).await.state,
            State::Failed(if cancel {
                Failure::Cancelled
            } else {
                Failure::AccountChanged
            })
        );
        assert!(db.credential_accounts(&namespace()).unwrap().is_empty());
        owner.shutdown().await.unwrap();
        drop(owner);
        drop(db);
        let db = Db::open(root.path()).unwrap();
        let owner = OpenAiAttempts::new(&db).unwrap();
        assert!(owner.status(&begun.id).is_err());
        assert!(db.credential_accounts(&namespace()).unwrap().is_empty());
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        owner.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn auth02_device_failure_cancel_and_expiry_stop_polling_without_fake_success() {
    for (status, outcome) in [
        (401, State::Failed(Failure::Remote)),
        (403, State::Failed(Failure::Cancelled)),
        (404, State::Expired),
    ] {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let server = issuer(
            vec![
                (
                    200,
                    json!({"device_auth_id":"device","user_code":"code","interval":"-1"}),
                ),
                (status, json!({})),
            ],
            None,
        )
        .await;
        let mut owner = owner(&db, server.auth);
        if outcome == State::Expired {
            owner.settings.lifetime = Duration::from_secs(1);
        }
        let begun = owner
            .begin(OAuthMethod::Device, "headless".into())
            .await
            .unwrap();
        until(|| server.requests.lock().unwrap().len() == 2).await;
        if outcome == State::Failed(Failure::Cancelled) {
            owner.cancel(&begun.id).await.unwrap();
        }
        assert_eq!(terminal(&owner, &begun.id).await.state, outcome);
        server.worker.await.unwrap();
        assert_eq!(server.requests.lock().unwrap().len(), 2);
        assert!(db.credential_accounts(&namespace()).unwrap().is_empty());
        owner.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn auth01_attempt_admission_is_bounded_and_shutdown_joins_pending_work() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut auth = OpenAiAuth::new().unwrap();
    auth.issuer = format!("http://{}", listener.local_addr().unwrap());
    let owner = owner(&db, auth);
    for index in 0..MAX_ATTEMPTS {
        owner
            .begin(OAuthMethod::Device, format!("pending-{index}"))
            .await
            .unwrap();
    }
    assert!(
        owner
            .begin(OAuthMethod::Device, "overflow".into())
            .await
            .is_err()
    );
    assert_eq!(owner.entries.lock().unwrap().len(), MAX_ATTEMPTS);
    owner.shutdown().await.unwrap();
    assert!(owner.entries.lock().unwrap().is_empty());
    assert!(
        owner
            .begin(OAuthMethod::Device, "closed".into())
            .await
            .is_err()
    );
    assert!(db.credential_accounts(&namespace()).unwrap().is_empty());
}
