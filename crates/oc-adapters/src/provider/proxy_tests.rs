//! Real offline proxy hops, with process-local trust and environment isolation.
use super::*;
use crate::discovery::DiscoveryClient as _;
use base64::Engine as _;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, atomic::AtomicUsize};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

const CHILD: &str = "OC_NETWORK_PROXY_CHILD";
const NAME: &str = "provider::proxy_tests::w4_system_proxy_routes_and_no_proxy";
const SERVER: &[u8] = include_bytes!("../../../../fixtures/network-proxy-server.pem");

fn acceptor(pem: &[u8]) -> tokio_rustls::TlsAcceptor {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject as _};
    tokio_rustls::TlsAcceptor::from(Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from_pem_slice(pem).unwrap()],
                PrivateKeyDer::from_pem_slice(pem).unwrap(),
            )
            .unwrap(),
    ))
}

async fn http(socket: &mut (impl AsyncRead + Unpin)) -> Option<(String, Vec<u8>)> {
    let mut bytes = Vec::new();
    loop {
        bytes.push(socket.read_u8().await.ok()?);
        assert!(bytes.len() <= 16 * 1024);
        if bytes.ends_with(b"\r\n\r\n") {
            let head = String::from_utf8(bytes).unwrap();
            let size = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|value| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            assert!(size <= 1024 * 1024);
            let mut body = vec![0; size];
            socket.read_exact(&mut body).await.ok()?;
            return Some((head, body));
        }
    }
}

#[derive(Default)]
struct Seen {
    proxies: usize,
    connects: usize,
    frames: usize,
    requests: Vec<String>,
}

async fn origin(mut socket: impl AsyncRead + AsyncWrite + Unpin, seen: Arc<Mutex<Seen>>) {
    let Some((head, body)) = http(&mut socket).await else {
        return;
    };
    assert!(!head.to_ascii_lowercase().contains("proxy-authorization:"));
    let first = head.lines().next().unwrap().to_owned();
    seen.lock().unwrap().requests.push(first.clone());
    let completed = json!({"type":"response.completed","response":{"id":"proxy-response","status":"completed","output":[],"usage":{"input_tokens":1,"output_tokens":1}}});
    if let Some(key) = head.lines().find_map(|line| {
        line.split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("sec-websocket-key"))
            .map(|(_, value)| value.trim())
    }) {
        let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
        socket.write_all(format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").as_bytes()).await.unwrap();
        let mut ws = tokio_tungstenite::WebSocketStream::from_raw_socket(
            socket,
            tokio_tungstenite::tungstenite::protocol::Role::Server,
            None,
        )
        .await;
        let frame = ws.next().await.unwrap().unwrap();
        let frame: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
        assert_eq!(frame["type"], "response.create");
        seen.lock().unwrap().frames += 1;
        ws.send(tokio_tungstenite::tungstenite::Message::text(
            json!({"type":"response.created","response":{"id":"proxy-response","status":"in_progress"}}).to_string(),
        )).await.unwrap();
        ws.send(tokio_tungstenite::tungstenite::Message::text(
            completed.to_string(),
        ))
        .await
        .unwrap();
        let _ = ws.next().await;
        let _ = ws.close(None).await;
        return;
    }
    let (status, content_type, payload) = if first.contains("/responses ") {
        (
            "200 OK",
            "text/event-stream",
            format!("data: {completed}\n\n"),
        )
    } else if first.contains("/models ") {
        ("200 OK", "application/json", "{\"data\":[]}".into())
    } else if first.contains("/mcp?token=fixture ") {
        let rpc: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let result = match rpc["method"].as_str() {
            Some("initialize") => {
                json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"proxy-fixture","version":"1"}})
            }
            Some("tools/list") => json!({"tools":[]}),
            _ => Value::Null,
        };
        if !result.is_null() {
            (
                "200 OK",
                "application/json",
                json!({"jsonrpc":"2.0","id":rpc["id"],"result":result}).to_string(),
            )
        } else {
            (
                if first.starts_with("GET ") {
                    "405 Method Not Allowed"
                } else {
                    "202 Accepted"
                },
                "application/json",
                String::new(),
            )
        }
    } else {
        assert!(first.contains("/page "));
        ("200 OK", "text/plain", "proxy page".into())
    };
    let _ = socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).as_bytes()).await;
}

async fn proxy(
    mut socket: impl AsyncRead + AsyncWrite + Unpin,
    target: std::net::SocketAddr,
    seen: Arc<Mutex<Seen>>,
) {
    let Some((head, body)) = http(&mut socket).await else {
        return;
    };
    let expected = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("user@proxy:p:ss")
    );
    assert_eq!(
        head.lines().find_map(|line| line
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("proxy-authorization"))
            .map(|(_, value)| value.trim())),
        Some(expected.as_str())
    );
    let mut upstream = tokio::net::TcpStream::connect(target).await.unwrap();
    if head.starts_with("CONNECT ") {
        assert!(head.starts_with(&format!(
            "CONNECT proxy-only.invalid:{} HTTP/1.1\r\n",
            target.port()
        )));
        seen.lock().unwrap().connects += 1;
        socket
            .write_all(b"HTTP/1.1 200 Connection established\r\nX-Proxy-Note: \xff\r\n\r\n")
            .await
            .unwrap();
    } else {
        let mut parts = head.lines().next().unwrap().split_whitespace();
        let method = parts.next().unwrap();
        let url = reqwest::Url::parse(parts.next().unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("proxy-only.invalid"));
        let path = url.query().map_or_else(
            || url.path().to_owned(),
            |query| format!("{}?{query}", url.path()),
        );
        let mut forwarded = format!("{method} {path} HTTP/1.1\r\n");
        for line in head.lines().skip(1).filter(|line| !line.is_empty()) {
            if !line
                .to_ascii_lowercase()
                .starts_with("proxy-authorization:")
            {
                forwarded.push_str(line);
                forwarded.push_str("\r\n");
            }
        }
        forwarded.push_str("\r\n");
        upstream.write_all(forwarded.as_bytes()).await.unwrap();
        upstream.write_all(&body).await.unwrap();
    }
    let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
}

async fn child(case: &str) {
    let base = std::env::var("OC_NETWORK_PROXY_BASE").unwrap();
    let is_ws = case.starts_with("ws") || case == "proxy-certificate";
    let root = tempfile::tempdir().unwrap();
    let db = crate::storage::Db::open(root.path().join("data").as_path()).unwrap();
    db.create_session("proxy-session").unwrap();
    let channels = websocket::Channels::default();
    let mut config = ResponsesConfig {
        base_url: base.clone(),
        api_key: "provider-fixture".into(),
        timeout: Some(false),
        header_timeout_ms: Some(2000),
        chunk_timeout_ms: Some(2000),
        connect_timeout: Duration::from_millis(700),
        allow_private: false,
        headers: BTreeMap::new(),
        cache: CacheMode::None,
        wire: Default::default(),
    };
    config.wire.endpoint = Some(
        crate::endpoint::EndpointBinding::admit(&base, case == "no-proxy", "proxy fixture")
            .unwrap(),
    );
    if is_ws {
        config.wire.openai = Some(crate::auth::OpenAiBinding {
            capture: None,
            scope: "proxy-fixture".into(),
            subscription: false,
            account: None,
        });
        config.wire.channels = Some(channels.clone());
        config = config.with_context(
            context::RequestContext::capture(&db, root.path(), "proxy-session").unwrap(),
        );
    }
    let sends = AtomicUsize::new(0);
    let result = stream_input_counted(
        &config,
        "fixture",
        None,
        &[],
        &[],
        64,
        &AtomicBool::new(false),
        &mut |_| {},
        &mut || async {
            sends.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    )
    .await;
    channels.shutdown().await.unwrap();
    if case == "proxy-certificate" {
        assert!(
            matches!(result, Err(ProviderError::InvalidCertificate)),
            "{result:?}"
        );
        assert_eq!(
            sends.load(Ordering::SeqCst),
            0,
            "certificate refusal must not fall back to HTTP"
        );
        return;
    }
    assert_eq!(result.unwrap().finish, FinishReason::Stop);
    assert_eq!(sends.load(Ordering::SeqCst), 1);
    if is_ws {
        return;
    }
    config.wire.endpoint = Some(
        crate::endpoint::EndpointBinding::admit(&base, true, "trusted proxy fixture").unwrap(),
    );
    stream_generation(
        &config,
        "fixture",
        None,
        "proxy fixture",
        &[],
        &AtomicBool::new(false),
        None,
    )
    .await
    .unwrap();
    let discovery = crate::discovery::ReqwestDiscoveryClient::captured(&config).unwrap();
    assert_eq!(
        discovery
            .get(
                &format!("{base}/models"),
                &reqwest::header::HeaderMap::new(),
                Duration::from_secs(2)
            )
            .await
            .unwrap(),
        (200, b"{\"data\":[]}".to_vec())
    );
    assert_eq!(
        crate::webfetch::fetch(
            &format!("{base}/page"),
            None,
            crate::webfetch::FetchOptions::default()
        )
        .await
        .unwrap()
        .text,
        "proxy page"
    );
    let mcp =
        crate::mcp_remote::CodexWebClient::connect_remote(&crate::mcp_remote::CodexWebConfig {
            url: format!("{base}/mcp?token=fixture"),
            bearer: "mcp-fixture".into(),
            custom_headers: Default::default(),
            protocol: "legacy".into(),
            timeout: Duration::from_secs(2),
            startup_timeout: None,
            catalog_timeout: None,
        })
        .await
        .unwrap();
    assert!(
        mcp.list_tools(&AtomicBool::new(false))
            .await
            .unwrap()
            .is_empty()
    );
    mcp.close().await.unwrap();
}

#[tokio::test]
async fn w4_system_proxy_routes_and_no_proxy() {
    if let Ok(case) = std::env::var(CHILD) {
        child(&case).await;
        return;
    }
    let mut failures = Vec::new();
    for case in [
        "http",
        "https",
        "no-proxy",
        "ws",
        "ws-plain",
        "proxy-certificate",
    ] {
        let plain_origin = matches!(case, "https" | "ws-plain");
        let secure_proxy = matches!(case, "https" | "ws" | "proxy-certificate");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_port = proxy_listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let observed = seen.clone();
        let (stop, mut stopping) = tokio::sync::oneshot::channel::<()>();
        let peer = tokio::spawn(async move {
            let tls = acceptor(SERVER);
            let proxy_tls = acceptor(if case == "proxy-certificate" {
                include_bytes!("../../../../fixtures/provider-untrusted-tls.pem")
            } else {
                SERVER
            });
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut stopping => break,
                    Some(done) = tasks.join_next(), if !tasks.is_empty() => { done.unwrap(); }
                    next = listener.accept() => {
                        let (socket,_) = next.unwrap();
                        let tls = tls.clone(); let seen = observed.clone();
                        tasks.spawn(async move {
                            if plain_origin { origin(socket,seen).await; }
                            else if let Ok(socket) = tls.accept(socket).await { origin(socket,seen).await; }
                        });
                    }
                    next = proxy_listener.accept() => {
                        let (socket,_) = next.unwrap(); observed.lock().unwrap().proxies += 1;
                        let tls = proxy_tls.clone(); let seen = observed.clone();
                        tasks.spawn(async move {
                            if secure_proxy { if let Ok(socket) = tls.accept(socket).await { proxy(socket,target,seen).await; } }
                            else { proxy(socket,target,seen).await; }
                        });
                    }
                }
            }
            tasks.abort_all();
            while let Some(result) = tasks.join_next().await {
                if let Err(error) = result {
                    assert!(error.is_cancelled(), "{error}");
                }
            }
        });
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([NAME, "--exact", "--test-threads=4"])
            .env(CHILD, case)
            .kill_on_drop(true);
        for name in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
            "NO_PROXY",
            "no_proxy",
            "WS_PROXY",
            "ws_proxy",
            "WSS_PROXY",
            "wss_proxy",
            "REQUEST_METHOD",
        ] {
            command.env_remove(name);
        }
        command
            .env(
                "SSL_CERT_FILE",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../fixtures/network-proxy-ca.pem"
                ),
            )
            .env("SSL_CERT_DIR", "");
        let base = format!(
            "{}://{}:{}/v1",
            if plain_origin { "http" } else { "https" },
            if case == "no-proxy" {
                "localhost"
            } else {
                "proxy-only.invalid"
            },
            target.port()
        );
        command.env("OC_NETWORK_PROXY_BASE", base);
        let proxy = format!(
            "{}://user%40proxy:p%3Ass@{}:{proxy_port}",
            if secure_proxy { "https" } else { "http" },
            if secure_proxy {
                "localhost"
            } else {
                "127.0.0.1"
            }
        );
        command.env(
            if case == "http" {
                "all_proxy"
            } else if plain_origin {
                "HTTP_PROXY"
            } else {
                "HTTPS_PROXY"
            },
            &proxy,
        );
        if matches!(case, "ws" | "proxy-certificate") {
            command.env("WSS_PROXY", &proxy);
        }
        if case == "ws" {
            command.env("HTTPS_PROXY", "invalid:");
        }
        if case == "no-proxy" {
            command.env("NO_PROXY", "localhost");
        }
        let output = tokio::time::timeout(Duration::from_secs(8), command.output())
            .await
            .unwrap()
            .unwrap();
        let _ = stop.send(());
        peer.await.unwrap();
        if !output.status.success() {
            failures.push(case);
            eprintln!(
                "{case}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            continue;
        }
        let seen = seen.lock().unwrap();
        if case == "proxy-certificate" {
            assert_eq!(seen.proxies, 1);
            assert!(seen.requests.is_empty());
        } else if case.starts_with("ws") {
            assert_eq!(seen.frames, 1);
            assert_eq!(seen.connects, 1);
            assert_eq!(seen.requests.len(), 1, "no HTTP fallback");
        } else {
            assert_eq!(seen.frames, 0);
            assert!(
                seen.requests
                    .iter()
                    .any(|r| r.contains("/mcp?token=fixture "))
            );
            if case == "no-proxy" {
                assert_eq!(seen.proxies, 0);
            } else {
                assert!(seen.proxies >= 5);
            }
        }
    }
    assert!(failures.is_empty(), "proxy cases failed: {failures:?}");
}
