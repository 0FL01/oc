//! Owned, ephemeral built-in OpenAI authorization. No browser/profile imports,
//! foreign port probes, persistent attempts or retry of a token exchange.
use super::{AuthError, AuthScope, OpenAiAuth, openai::*};
use crate::storage::Db;
use base64::Engine;
use oc_core::queries::{
    AuthAttempt, AuthAttemptFailure as Failure, AuthAttemptState as State, OAuthMethod,
};
use serde::Deserialize;
use sha2::Digest;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinHandle,
    time::Instant,
};

const MAX_ATTEMPTS: usize = 8;
const CALLBACK_BYTES: usize = 8192;

#[derive(Clone)]
struct Settings {
    lifetime: Duration,
    retention: Duration,
    cleanup: Duration,
    ports: [u16; 2],
    bind_delay: Duration,
    polling_margin: Duration,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            lifetime: Duration::from_secs(600),
            retention: Duration::from_secs(60),
            cleanup: Duration::from_secs(30),
            ports: [1455, 1457],
            bind_delay: Duration::from_millis(200),
            polling_margin: Duration::from_secs(3),
        }
    }
}

struct Attempt {
    view: AuthAttempt,
    deadline: Instant,
    ended: Option<Instant>,
    cancel: watch::Sender<bool>,
    worker: Option<JoinHandle<()>>,
}
type Attempts = Arc<Mutex<BTreeMap<String, Arc<Mutex<Attempt>>>>>;

/// One application-owned registry. Drop cancels/aborts; acknowledged shutdown
/// joins every worker and releases its listeners/HTTP futures before returning.
pub struct OpenAiAttempts {
    db: Db,
    auth: Arc<OpenAiAuth>,
    entries: Attempts,
    maintenance: Arc<Mutex<Option<JoinHandle<()>>>>,
    settings: Settings,
    closing: AtomicBool,
}
impl OpenAiAttempts {
    pub fn new(db: &Db) -> Result<Self, AuthError> {
        Ok(Self {
            db: db.shared_handle(),
            auth: Arc::new(OpenAiAuth::new()?),
            entries: Arc::new(Mutex::new(BTreeMap::new())),
            maintenance: Arc::new(Mutex::new(None)),
            settings: Settings::default(),
            closing: AtomicBool::new(false),
        })
    }

    pub async fn begin(
        &self,
        method: OAuthMethod,
        label: String,
    ) -> Result<AuthAttempt, AuthError> {
        if self.closing.load(Ordering::Acquire)
            || label.trim().is_empty()
            || label.len() > 128
            || label.chars().any(char::is_control)
        {
            return Err(AuthError::Conflict);
        }
        // Only this owner's known browser attempt can be replaced. Never dial
        // /cancel or signal the process occupying a preferred port.
        if method == OAuthMethod::Browser {
            let owned: Vec<String> = self
                .entries
                .lock()
                .expect("auth attempts")
                .values()
                .filter_map(|entry| {
                    let entry = entry.lock().expect("auth attempt");
                    (entry.view.method == method && entry.view.state == State::Pending)
                        .then(|| entry.view.id.clone())
                })
                .collect();
            for id in owned {
                self.cancel(&id).await?;
            }
        }
        let namespace = AuthScope::admit("openai", OPENAI_BASE_URL)?
            .namespace()
            .to_owned();
        let epoch = self.db.credential_epoch(&namespace);
        let id = random(16)?;
        let now = unix_seconds();
        let view = AuthAttempt {
            id: id.clone(),
            method,
            state: State::Pending,
            url: None,
            instructions: None,
            created_at: now,
            expires_at: now.saturating_add(self.settings.lifetime.as_secs() as i64),
            account_id: None,
        };
        let (cancel, mut cancelled) = watch::channel(false);
        let entry = Arc::new(Mutex::new(Attempt {
            view: view.clone(),
            deadline: Instant::now() + self.settings.lifetime,
            ended: None,
            cancel,
            worker: None,
        }));
        let mut entries = self.entries.lock().expect("auth attempts");
        if self.closing.load(Ordering::Acquire) || entries.len() >= MAX_ATTEMPTS {
            return Err(AuthError::Conflict);
        }
        entries.insert(id, entry.clone());
        let auth = self.auth.clone();
        let settings = self.settings.clone();
        let db = self.db.shared_handle();
        let task_entry = entry.clone();
        let deadline = entry.lock().expect("auth attempt").deadline;
        let worker = tokio::spawn(async move {
            let exchange = async {
                match method {
                    OAuthMethod::Browser => browser(&auth, &task_entry, &settings).await,
                    OAuthMethod::Device => device(&auth, &task_entry, &settings).await,
                }
            };
            let result = tokio::select! {
                biased;
                _ = cancelled.changed() => Err(State::Failed(Failure::Cancelled)),
                _ = tokio::time::sleep_until(deadline) => Err(State::Expired),
                result = exchange => result.map_err(State::Failed),
            };
            let mut callback = None;
            let (complete, detail) = {
                let mut current = task_entry.lock().expect("auth attempt");
                if current.view.state == State::Pending {
                    let state = match result {
                        Ok(authorized) => {
                            callback = authorized.callback;
                            if Instant::now() >= current.deadline {
                                State::Expired
                            } else {
                                match authorized.tokens.material(method.id(), unix_seconds()) {
                                    Ok(material) => match db.add_credential_if_epoch(
                                        &namespace, &label, material, epoch,
                                    ) {
                                        Ok(Some(account)) => {
                                            current.view.account_id = Some(account.id);
                                            State::Complete
                                        }
                                        Ok(None) => State::Failed(Failure::AccountChanged),
                                        Err(_) => State::Failed(Failure::Unavailable),
                                    },
                                    Err(_) => State::Failed(Failure::InvalidTokens),
                                }
                            }
                        }
                        Err(state) => state,
                    };
                    finish(&mut current, state);
                }
                let detail = match current.view.state {
                    State::Failed(failure) => failure.message(),
                    State::Expired => "Authorization expired",
                    _ => "",
                };
                (current.view.state == State::Complete, detail)
            };
            if let Some(mut socket) = callback {
                // The success page follows the durable acknowledgement, never
                // just arrival of a code. A failed/cancelled exchange cannot lie.
                let _ = reply(
                    &mut socket,
                    if complete { 200 } else { 400 },
                    &page(complete, detail),
                )
                .await;
            }
        });
        entry.lock().expect("auth attempt").worker = Some(worker);
        drop(entries);
        self.maintain();
        Ok(view)
    }

    pub fn status(&self, id: &str) -> Result<AuthAttempt, AuthError> {
        let entries = self.entries.lock().expect("auth attempts");
        let entry = entries.get(id).ok_or(AuthError::Conflict)?;
        Ok(entry.lock().expect("auth attempt").view.clone())
    }

    pub async fn cancel(&self, id: &str) -> Result<AuthAttempt, AuthError> {
        let entry = self
            .entries
            .lock()
            .expect("auth attempts")
            .get(id)
            .cloned()
            .ok_or(AuthError::Conflict)?;
        let worker = {
            let mut current = entry.lock().expect("auth attempt");
            if current.view.state == State::Pending {
                finish(&mut current, State::Failed(Failure::Cancelled));
                let _ = current.cancel.send(true);
            }
            current.worker.take()
        };
        if let Some(worker) = worker {
            worker.await.map_err(|_| AuthError::Remote)?;
        }
        let view = entry.lock().expect("auth attempt").view.clone();
        Ok(view)
    }

    pub async fn shutdown(&self) -> Result<(), AuthError> {
        self.closing.store(true, Ordering::Release);
        let timer = self.maintenance.lock().expect("auth cleanup").take();
        if let Some(timer) = timer {
            timer.abort();
            // An intentional timer abort is not a failed authorization cleanup.
            let _ = timer.await;
        }
        let ids: Vec<String> = self
            .entries
            .lock()
            .expect("auth attempts")
            .keys()
            .cloned()
            .collect();
        let mut failed = false;
        for id in ids {
            failed |= self.cancel(&id).await.is_err();
        }
        self.entries.lock().expect("auth attempts").clear();
        if failed {
            Err(AuthError::Remote)
        } else {
            Ok(())
        }
    }

    fn maintain(&self) {
        let mut timer = self.maintenance.lock().expect("auth cleanup");
        if timer.as_ref().is_some_and(|task| !task.is_finished()) {
            return;
        }
        let entries = self.entries.clone();
        let settings = self.settings.clone();
        let maintenance = self.maintenance.clone();
        *timer = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(settings.cleanup).await;
                let mut entries = entries.lock().expect("auth attempts");
                entries.retain(|_, entry| {
                    let mut current = entry.lock().expect("auth attempt");
                    if current.view.state == State::Pending
                        && current.worker.as_ref().is_some_and(JoinHandle::is_finished)
                    {
                        finish(&mut current, State::Failed(Failure::Unavailable));
                    }
                    let old = current
                        .ended
                        .is_some_and(|ended| ended.elapsed() >= settings.retention);
                    !old || current
                        .worker
                        .as_ref()
                        .is_some_and(|task| !task.is_finished())
                });
                if entries.is_empty() {
                    // Publish idle while holding the entries lock, so a new begin
                    // cannot attach to a timer that has already decided to stop.
                    maintenance.lock().expect("auth cleanup").take();
                    break;
                }
            }
        }));
    }
}
impl Drop for OpenAiAttempts {
    fn drop(&mut self) {
        if let Some(timer) = self.maintenance.lock().expect("auth cleanup").take() {
            timer.abort();
        }
        for entry in self.entries.lock().expect("auth attempts").values() {
            let current = entry.lock().expect("auth attempt");
            let _ = current.cancel.send(true);
            if let Some(worker) = &current.worker {
                worker.abort();
            }
        }
    }
}

fn finish(entry: &mut Attempt, state: State) {
    entry.view.state = state;
    entry.view.url = None;
    entry.view.instructions = None;
    entry.ended = Some(Instant::now());
}
fn present(entry: &Arc<Mutex<Attempt>>, url: String, instructions: String) {
    let mut entry = entry.lock().expect("auth attempt");
    if entry.view.state == State::Pending {
        entry.view.url = Some(url);
        entry.view.instructions = Some(instructions);
    }
}
fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
fn random(bytes: usize) -> Result<String, AuthError> {
    let mut data = vec![0; bytes];
    // SAFETY: this exclusively owned initialized buffer is writable for its length.
    let n = unsafe { libc::getrandom(data.as_mut_ptr().cast(), data.len(), 0) };
    if n != data.len() as isize {
        return Err(AuthError::Remote);
    }
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data))
}
fn failure(error: AuthError) -> Failure {
    match error {
        AuthError::InvalidTokens => Failure::InvalidTokens,
        _ => Failure::Remote,
    }
}

struct Authorized {
    tokens: Tokens,
    callback: Option<TcpStream>,
}

async fn browser(
    auth: &OpenAiAuth,
    entry: &Arc<Mutex<Attempt>>,
    settings: &Settings,
) -> Result<Authorized, Failure> {
    let verifier = random(32).map_err(failure)?;
    let state = random(32).map_err(failure)?;
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()));
    let mut listener = None;
    for port in settings.ports {
        for attempt in 0..10 {
            match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
                Ok(bound) => {
                    listener = Some(bound);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                    if attempt < 9 {
                        tokio::time::sleep(settings.bind_delay).await;
                    }
                }
                Err(_) => return Err(Failure::Unavailable),
            }
        }
        if listener.is_some() {
            break;
        }
    }
    let listener = listener.ok_or(Failure::Unavailable)?;
    let port = listener
        .local_addr()
        .map_err(|_| Failure::Unavailable)?
        .port();
    let redirect = format!("http://localhost:{port}/auth/callback");
    let mut url = reqwest::Url::parse(&format!("{}/oauth/authorize", auth.issuer))
        .map_err(|_| Failure::Unavailable)?;
    url.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect.as_str()),
        ("scope", "openid profile email offline_access"),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state.as_str()),
        ("originator", "opencode"),
    ]);
    present(
        entry,
        url.into(),
        "Complete authorization in your browser. This window will close automatically.".into(),
    );
    loop {
        let (mut socket, _) = listener.accept().await.map_err(|_| Failure::Unavailable)?;
        let target = match tokio::time::timeout(
            Duration::from_secs(2),
            callback_target(&mut socket),
        )
        .await
        {
            Ok(Ok(target)) => target,
            _ => {
                let _ = reply(&mut socket, 400, &page(false, "Invalid callback request")).await;
                continue;
            }
        };
        if target.path() != "/auth/callback" {
            let _ = reply(&mut socket, 404, "Not found").await;
            continue;
        }
        let mut code = None;
        let mut received_state = None;
        let mut rejected = false;
        for (key, value) in target.query_pairs() {
            match key.as_ref() {
                "code" if code.is_none() => code = Some(value.into_owned()),
                "state" if received_state.is_none() => received_state = Some(value.into_owned()),
                "code" | "state" | "error" | "error_description" => rejected = true,
                _ => {}
            }
        }
        let invalid = if rejected
            || code
                .as_deref()
                .is_none_or(|code| !bounded_field(code, 4096))
        {
            Some(Failure::Callback)
        } else if received_state.as_deref() != Some(state.as_str()) {
            Some(Failure::StateMismatch)
        } else {
            None
        };
        if let Some(invalid) = invalid {
            let _ = reply(&mut socket, 400, &page(false, invalid.message())).await;
            return Err(invalid);
        }
        let tokens = auth
            .tokens(&[
                ("grant_type", "authorization_code"),
                ("client_id", CLIENT_ID),
                ("code", code.as_deref().unwrap_or_default()),
                ("redirect_uri", &redirect),
                ("code_verifier", &verifier),
            ])
            .await;
        match tokens {
            Ok(tokens) => {
                return Ok(Authorized {
                    tokens,
                    callback: Some(socket),
                });
            }
            Err(error) => {
                let error = failure(error);
                let _ = reply(&mut socket, 400, &page(false, error.message())).await;
                return Err(error);
            }
        }
    }
}

async fn callback_target(socket: &mut TcpStream) -> Result<reqwest::Url, ()> {
    let mut data = Vec::new();
    loop {
        let mut buf = [0; 1024];
        let n = socket.read(&mut buf).await.map_err(|_| ())?;
        if n == 0 || data.len() + n > CALLBACK_BYTES {
            return Err(());
        }
        data.extend_from_slice(&buf[..n]);
        if data.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            break;
        }
    }
    let line = std::str::from_utf8(&data)
        .map_err(|_| ())?
        .lines()
        .next()
        .ok_or(())?;
    let mut parts = line.split_whitespace();
    if parts.next() != Some("GET") {
        return Err(());
    }
    let path = parts.next().ok_or(())?;
    if !path.starts_with('/')
        || path.starts_with("//")
        || parts.next() != Some("HTTP/1.1")
        || parts.next().is_some()
    {
        return Err(());
    }
    reqwest::Url::parse(&format!("http://localhost{path}")).map_err(|_| ())
}

#[derive(Deserialize)]
struct DeviceCode {
    device_auth_id: String,
    user_code: String,
    interval: Option<String>,
}
#[derive(Deserialize)]
struct DeviceToken {
    authorization_code: String,
    code_verifier: String,
}

async fn device(
    auth: &OpenAiAuth,
    entry: &Arc<Mutex<Attempt>>,
    settings: &Settings,
) -> Result<Authorized, Failure> {
    let response = auth
        .post("/api/accounts/deviceauth/usercode")
        .await
        .map_err(failure)?
        .json(&serde_json::json!({"client_id":CLIENT_ID}))
        .send()
        .await
        .map_err(|_| Failure::Remote)?;
    auth.check_peer(&response).map_err(failure)?;
    if !response.status().is_success() {
        return Err(Failure::Remote);
    }
    let code: DeviceCode = OpenAiAuth::json(response).await.map_err(failure)?;
    if !bounded_field(&code.device_auth_id, 4096) || !bounded_field(&code.user_code, 128) {
        return Err(Failure::InvalidTokens);
    }
    let delay = device_interval(code.interval.as_deref()) + settings.polling_margin;
    present(
        entry,
        format!("{}/codex/device", auth.issuer),
        format!("Enter code: {}", code.user_code),
    );
    loop {
        let response = auth.post("/api/accounts/deviceauth/token").await.map_err(failure)?
            .json(&serde_json::json!({"device_auth_id":code.device_auth_id,"user_code":code.user_code}))
            .send().await.map_err(|_| Failure::Remote)?;
        auth.check_peer(&response).map_err(failure)?;
        if response.status().is_success() {
            let token: DeviceToken = OpenAiAuth::json(response).await.map_err(failure)?;
            if !bounded_field(&token.authorization_code, 4096)
                || !bounded_field(&token.code_verifier, 4096)
            {
                return Err(Failure::InvalidTokens);
            }
            let redirect = format!("{}/deviceauth/callback", auth.issuer);
            let tokens = auth
                .tokens(&[
                    ("grant_type", "authorization_code"),
                    ("client_id", CLIENT_ID),
                    ("code", &token.authorization_code),
                    ("redirect_uri", &redirect),
                    ("code_verifier", &token.code_verifier),
                ])
                .await
                .map_err(failure)?;
            return Ok(Authorized {
                tokens,
                callback: None,
            });
        }
        if !matches!(response.status().as_u16(), 403 | 404) {
            return Err(Failure::Remote);
        }
        drop(response);
        tokio::time::sleep(delay).await;
    }
}
fn bounded_field(value: &str, cap: usize) -> bool {
    !value.trim().is_empty() && value.len() <= cap && !value.chars().any(char::is_control)
}
fn device_interval(value: Option<&str>) -> Duration {
    // Donor Number.parseInt semantics on a structured interval field, not a
    // heuristic parser of free text. Zero/invalid default to five, negatives to one.
    let value = value.unwrap_or_default().trim_start();
    let sign = usize::from(value.starts_with(['+', '-']));
    let count = value[sign..].bytes().take_while(u8::is_ascii_digit).count();
    let parsed = value
        .get(..sign + count)
        .and_then(|v| v.parse::<i64>().ok());
    let seconds = parsed.filter(|v| *v != 0).unwrap_or(5).clamp(1, 600);
    Duration::from_secs(seconds as u64)
}

fn page(complete: bool, detail: &str) -> String {
    let title = if complete {
        "Authorization successful"
    } else {
        "Authorization failed"
    };
    let message = if complete {
        "OpenCode is now connected to ChatGPT."
    } else {
        "OpenCode couldn't finish connecting to ChatGPT."
    };
    let footnote = if complete {
        "You can close this window."
    } else {
        "Close this window and try again from OpenCode."
    };
    let detail = detail
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    let script = if complete {
        "<script>setTimeout(function(){try{window.close()}catch(e){}},2500)</script>"
    } else {
        ""
    };
    include_str!("callback.html")
        .replace("$TITLE", title)
        .replace("$STATUS", if complete { "success" } else { "error" })
        .replace("$MESSAGE", message)
        .replace("$FOOTNOTE", footnote)
        .replace("$HIDDEN", if detail.is_empty() { " hidden" } else { "" })
        .replace("$SCRIPT", script)
        // Insert escaped detail last so template-like input is never reprocessed.
        .replace("$DETAIL", &detail)
}
async fn reply(socket: &mut TcpStream, status: u16, body: &str) -> Result<(), ()> {
    let response = format!(
        "HTTP/1.1 {status} Status\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    tokio::time::timeout(
        Duration::from_secs(2),
        socket.write_all(response.as_bytes()),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}

#[cfg(test)]
mod tests;
