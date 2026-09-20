//! Webfetch tool for T11 (TOOL07–TOOL08).
//!
//! Plain `GET` with metadata-first results (`status`, `content-type`,
//! original URL), bounded readable text extraction (HTML/JSON/text), manual
//! redirect handling (per-hop SSRF re-check, auth never inherited), and
//! dial-time guards: DNS pre-check on every hop plus post-dial
//! `remote_addr` verification against DNS-rebinding flips. Loopback and
//! private ranges are refused unless the explicit test-only
//! `allow_loopback` exception is set; production callers leave it `false`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use thiserror::Error;

/// Response body cap (bytes retained; overflow flagged, never grown).
pub const BODY_CAP_BYTES: usize = 1024 * 1024;
/// Max redirect hops followed.
pub const MAX_REDIRECTS: usize = 5;

/// Typed fetch errors (no body contents, no credentials).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Malformed URL or unsupported scheme.
    #[error("invalid url")]
    InvalidUrl,
    /// SSRF guard: non-public dial target.
    #[error("private host refused")]
    PrivateHost,
    /// Too many redirects.
    #[error("too many redirects")]
    TooManyRedirects,
    /// Deadline exceeded (connect or total).
    #[error("deadline exceeded")]
    Deadline,
    /// Body exceeded the cap and was cut (returned only via `truncated`
    /// flag on success; this variant marks header-level refusal).
    #[error("body too large")]
    TooLarge,
    /// Transport failure (kind only).
    #[error("transport error")]
    Transport,
    /// Non-success status is returned as data, not here; this covers
    /// malformed responses.
    #[error("bad response")]
    BadResponse,
}

/// Per-call options. `allow_loopback` is the explicit test exception for
/// loopback destinations; it never defaults on.
#[derive(Debug, Clone, Copy)]
pub struct FetchOptions {
    /// Total deadline per call.
    pub timeout: Duration,
    /// TCP/TLS connect timeout.
    pub connect_timeout: Duration,
    /// Max redirect hops.
    pub max_redirects: usize,
    /// Body bytes retained.
    pub body_cap: usize,
    /// Test-only loopback exception.
    pub allow_loopback: bool,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            max_redirects: MAX_REDIRECTS,
            body_cap: BODY_CAP_BYTES,
            allow_loopback: false,
        }
    }
}

/// Metadata-first fetch result with bounded readable text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResult {
    /// HTTP status code.
    pub status: u16,
    /// Response content type (without parameters), if any.
    pub content_type: Option<String>,
    /// Original requested URL.
    pub url: String,
    /// Final URL after redirects.
    pub final_url: String,
    /// Extracted readable text (bounded).
    pub text: String,
    /// True when the body was cut at the cap.
    pub truncated: bool,
}

/// True when an IP is publicly routable (global unicast, not special).
///
/// IPv4-mapped IPv6 addresses unwrap to their v4 inner address first, so
/// `::ffff:10.0.0.1` is refused like `10.0.0.1`.
pub fn ip_is_public(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                return is_public_v6(v6);
            }
        },
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
                || v4 == Ipv4Addr::new(0, 0, 0, 0)
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 64))
        }
        IpAddr::V6(_) => true,
    }
}

fn is_public_v6(v6: Ipv6Addr) -> bool {
    // 2001:db8::/32 documentation range has no stable helper on 1.93.
    let documentation = v6.segments()[0] == 0x2001 && v6.segments()[1] == 0x0db8;
    !(v6.is_loopback()
        || v6.is_multicast()
        || v6.is_unspecified()
        || documentation
        || (v6.segments()[0] & 0xffc0) == 0xfe80
        || (v6.segments()[0] & 0xfe00) == 0xfc00)
}

/// Pre-dial guard: every resolved address of `host:port` must be public
/// (or loopback under the explicit test exception).
pub async fn check_host(host: &str, port: u16, allow_loopback: bool) -> Result<(), FetchError> {
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| FetchError::PrivateHost)?;
    let mut any = false;
    for addr in addrs {
        any = true;
        let ip = addr.ip();
        if allow_loopback && (ip.is_loopback() || ip == IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))) {
            continue;
        }
        if !ip_is_public(ip) {
            return Err(FetchError::PrivateHost);
        }
    }
    if any {
        Ok(())
    } else {
        Err(FetchError::PrivateHost)
    }
}

fn parse_url(url: &str) -> Result<(String, String, u16, String), FetchError> {
    let (scheme, rest) = url.split_once("://").ok_or(FetchError::InvalidUrl)?;
    if scheme != "http" && scheme != "https" {
        return Err(FetchError::InvalidUrl);
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        return Err(FetchError::InvalidUrl);
    }
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        // Bracketed IPv6 literal: [ip] or [ip]:port.
        match stripped.split_once("]:") {
            Some((ip, p)) => {
                let port = p.parse::<u16>().map_err(|_| FetchError::InvalidUrl)?;
                (format!("[{ip}]"), port)
            }
            None => match stripped.strip_suffix(']') {
                Some(ip) => (format!("[{ip}]"), default_port(scheme)),
                None => return Err(FetchError::InvalidUrl),
            },
        }
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) if !h.is_empty() => match p.parse::<u16>() {
                Ok(port) => (h.to_string(), port),
                Err(_) => (authority.to_string(), default_port(scheme)),
            },
            _ => (authority.to_string(), default_port(scheme)),
        }
    };
    if host.is_empty() {
        return Err(FetchError::InvalidUrl);
    }
    Ok((scheme.to_string(), host, port, path))
}

fn default_port(scheme: &str) -> u16 {
    if scheme == "https" { 443 } else { 80 }
}

fn redirect_target(location: &str, base: &str) -> Result<String, FetchError> {
    if location.contains('\0') || location.contains(' ') {
        return Err(FetchError::BadResponse);
    }
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_string());
    }
    if let Some(path) = location.strip_prefix('/') {
        let (scheme, host, port, _) = parse_url(base)?;
        let authority = if port == default_port(&scheme) {
            host
        } else {
            format!("{host}:{port}")
        };
        return Ok(format!("{scheme}://{authority}/{path}"));
    }
    Err(FetchError::BadResponse)
}

/// Fetch a URL with full guard rails.
///
/// `auth` (if any) is sent only on the first hop, never inherited by
/// redirects. Bodies stream with a byte cap; HTML is reduced to readable
/// text, JSON is pretty-printed when parseable.
pub async fn fetch(
    url: &str,
    auth: Option<&str>,
    opts: FetchOptions,
) -> Result<FetchResult, FetchError> {
    if auth.map(str::is_empty).unwrap_or(false) {
        return Err(FetchError::InvalidUrl);
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(opts.connect_timeout)
        .build()
        .map_err(|_| FetchError::Transport)?;

    let mut current = url.to_string();
    let mut hops = 0usize;
    loop {
        let (scheme, host, port, path) = parse_url(&current)?;
        check_host(&host, port, opts.allow_loopback).await?;
        let target = format!("{scheme}://{host}{}{path}", with_port(&scheme, port));

        let mut req = client.get(&target).timeout(opts.timeout);
        if hops == 0
            && let Some(token) = auth
        {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                FetchError::Deadline
            } else {
                FetchError::Transport
            }
        })?;
        // Post-dial rebinding guard: the connected peer must still be public
        // (or loopback under the test exception).
        if let Some(peer) = resp.remote_addr() {
            let ip = peer.ip();
            let loopback_ok = opts.allow_loopback && ip.is_loopback();
            if !loopback_ok && !ip_is_public(ip) {
                return Err(FetchError::PrivateHost);
            }
        }
        let status = resp.status().as_u16();
        if (300..400).contains(&status) {
            if hops >= opts.max_redirects {
                return Err(FetchError::TooManyRedirects);
            }
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(FetchError::BadResponse)?;
            current = redirect_target(location, &current)?;
            hops += 1;
            continue;
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.split(';').next().unwrap_or("").trim().to_lowercase())
            .filter(|ct| !ct.is_empty());
        let bytes = read_capped_body(resp, opts.body_cap).await?;
        let (text, truncated) = (
            extract_text(&bytes.body, content_type.as_deref()),
            bytes.truncated,
        );
        return Ok(FetchResult {
            status,
            content_type,
            url: url.to_string(),
            final_url: current,
            text,
            truncated,
        });
    }
}

fn with_port(scheme: &str, port: u16) -> String {
    if port == default_port(scheme) {
        String::new()
    } else {
        format!(":{port}")
    }
}

struct CappedBody {
    body: Vec<u8>,
    truncated: bool,
}

async fn read_capped_body(resp: reqwest::Response, cap: usize) -> Result<CappedBody, FetchError> {
    let mut body = Vec::new();
    let mut truncated = false;
    let mut resp = resp;
    loop {
        let chunk = resp.chunk().await.map_err(|e| {
            if e.is_timeout() {
                FetchError::Deadline
            } else {
                FetchError::Transport
            }
        })?;
        let Some(chunk) = chunk else {
            break;
        };
        if body.len() + chunk.len() > cap {
            let room = cap.saturating_sub(body.len());
            body.extend_from_slice(&chunk[..room]);
            truncated = true;
            break;
        }
        body.extend_from_slice(&chunk);
    }
    Ok(CappedBody { body, truncated })
}

/// Reduce a body to readable text by content type.
pub fn extract_text(bytes: &[u8], content_type: Option<&str>) -> String {
    let text = String::from_utf8_lossy(bytes);
    match content_type {
        Some("application/json") | Some("application/problem+json") => {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(value) => {
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.into_owned())
                }
                Err(_) => text.into_owned(),
            }
        }
        Some("text/html") => html_to_text(&text),
        _ => text.into_owned(),
    }
}

/// Minimal HTML→text: drops comments, `script`/`style` contents and tags,
/// decodes common entities, collapses whitespace.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    let mut skip_tag: Option<&str> = None;
    while i < bytes.len() {
        if skip_tag.is_none() && html[i..].starts_with("<!--") {
            if let Some(end) = html[i..].find("-->") {
                i += end + 3;
                continue;
            }
            break;
        }
        if bytes[i] == b'<' {
            let tag = tag_name(&html[i..]);
            if skip_tag.is_none() && (tag == "script" || tag == "style") {
                skip_tag = Some(if tag == "script" { "script" } else { "style" });
            } else if let Some(open) = skip_tag
                && tag == format!("/{open}")
            {
                skip_tag = None;
            }
            if let Some(end) = html[i..].find('>') {
                if skip_tag.is_none()
                    && (tag == "p"
                        || tag == "br"
                        || tag == "div"
                        || tag == "li"
                        || tag == "tr"
                        || tag.starts_with('h'))
                {
                    out.push('\n');
                }
                i += end + 1;
                continue;
            }
            break;
        }
        if skip_tag.is_some() {
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    collapse_entities(&out)
}

fn tag_name(fragment: &str) -> String {
    let inner: String = fragment
        .chars()
        .skip(1)
        .take_while(|c| *c != '>' && !c.is_whitespace())
        .collect();
    // Keep a possible leading `/` so closing tags compare: "/style".
    let inner = inner.trim_end_matches('/').to_string();
    inner.to_lowercase()
}

fn collapse_entities(text: &str) -> String {
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    let mut collapsed = String::with_capacity(text.len());
    let mut prev_space = true;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !prev_space {
            collapsed.push('\n');
        }
        collapsed.push_str(line);
        prev_space = false;
    }
    collapsed
}

#[cfg(test)]
mod tests {
    use super::{
        FetchError, FetchOptions, check_host, extract_text, fetch, html_to_text, ip_is_public,
    };
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Recorded inbound request (path + headers of interest).
    #[derive(Debug, Clone)]
    struct Seen {
        path: String,
        auth: Option<String>,
    }

    struct TestServer {
        base: String,
        seen: Arc<Mutex<Vec<Seen>>>,
        handle: tokio::task::JoinHandle<()>,
    }

    impl TestServer {
        async fn spawn() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let base = format!("http://{}", listener.local_addr().expect("addr"));
            let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
            let seen_task = seen.clone();
            let handle = tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    let seen = seen_task.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncReadExt as _;
                        use tokio::io::AsyncWriteExt as _;
                        let mut head = Vec::new();
                        let mut buf = [0u8; 4096];
                        loop {
                            match sock.read(&mut buf).await {
                                Ok(0) => return,
                                Ok(n) => {
                                    head.extend_from_slice(&buf[..n]);
                                    if head.len() > 65536
                                        || head.windows(4).any(|w| w == b"\r\n\r\n")
                                    {
                                        break;
                                    }
                                }
                                Err(_) => return,
                            }
                        }
                        let head_text = String::from_utf8_lossy(&head);
                        let mut lines = head_text.lines();
                        let request_line = lines.next().unwrap_or("");
                        let path = request_line
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or("/")
                            .to_string();
                        let mut headers = HashMap::new();
                        for line in lines {
                            if line.is_empty() {
                                break;
                            }
                            if let Some((k, v)) = line.split_once(':') {
                                headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                            }
                        }
                        seen.lock().expect("seen").push(Seen {
                            path: path.clone(),
                            auth: headers.get("authorization").cloned(),
                        });
                        let (status, extra, body, delay_ms) = route(&path);
                        if delay_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        }
                        let mut resp = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
                            body.len()
                        );
                        for (k, v) in extra {
                            resp.push_str(&format!("{k}: {v}\r\n"));
                        }
                        resp.push_str("\r\n");
                        let _ = sock.write_all(resp.as_bytes()).await;
                        let _ = sock.write_all(&body).await;
                    });
                }
            });
            Self { base, seen, handle }
        }

        fn shutdown(self) {
            self.handle.abort();
        }
    }

    /// Test routes: (status line, extra headers, body, delay ms).
    fn route(path: &str) -> (&'static str, Vec<(&'static str, String)>, Vec<u8>, u64) {
        match path {
            "/text" => ("200 OK", vec![("Content-Type", "text/plain".to_string())], b"hello world".to_vec(), 0),
            "/html" => (
                "200 OK",
                vec![("Content-Type", "text/html".to_string())],
                b"<html><head><style>.x{}</style><script>var x = 1;</script></head><body><h1>Hello &amp; bye</h1><p>World</p></body></html>".to_vec(),
                0,
            ),
            "/json" => (
                "200 OK",
                vec![("Content-Type", "application/json".to_string())],
                b"{\"key\":\"value\",\"n\":42}".to_vec(),
                0,
            ),
            "/redir" => ("302 Found", vec![("Location", "/text".to_string())], Vec::new(), 0),
            "/go-private" => (
                "302 Found",
                vec![("Location", "http://10.0.0.1/".to_string())],
                Vec::new(),
                0,
            ),
            "/big" => ("200 OK", vec![("Content-Type", "text/plain".to_string())], vec![b'x'; 3 * 1024 * 1024], 0),
            "/slow" => ("200 OK", vec![("Content-Type", "text/plain".to_string())], b"late".to_vec(), 5000),
            "/login" => ("302 Found", vec![("Location", "/check".to_string())], Vec::new(), 0),
            "/check" => ("200 OK", vec![("Content-Type", "text/plain".to_string())], b"checked".to_vec(), 0),
            _ => ("404 Not Found", vec![("Content-Type", "text/plain".to_string())], b"nope".to_vec(), 0),
        }
    }

    fn test_opts() -> FetchOptions {
        FetchOptions {
            timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(5),
            max_redirects: 5,
            body_cap: super::BODY_CAP_BYTES,
            allow_loopback: true,
        }
    }

    #[tokio::test]
    async fn tool07_text_html_json_redirect_status() {
        let server = TestServer::spawn().await;
        let text = fetch(&format!("{}/text", server.base), None, test_opts())
            .await
            .expect("text");
        assert_eq!(text.status, 200);
        assert_eq!(text.content_type.as_deref(), Some("text/plain"));
        assert_eq!(text.text, "hello world");
        assert!(!text.truncated);
        assert_eq!(text.url, format!("{}/text", server.base));

        let html = fetch(&format!("{}/html", server.base), None, test_opts())
            .await
            .expect("html");
        assert!(html.text.contains("Hello & bye"), "got: {}", html.text);
        assert!(html.text.contains("World"));
        assert!(!html.text.contains("var x"));

        let json = fetch(&format!("{}/json", server.base), None, test_opts())
            .await
            .expect("json");
        assert!(json.text.contains("\"key\": \"value\""));

        let redir = fetch(&format!("{}/redir", server.base), None, test_opts())
            .await
            .expect("redir");
        assert_eq!(redir.text, "hello world");
        assert_eq!(redir.final_url, format!("{}/text", server.base));

        let missing = fetch(&format!("{}/absent", server.base), None, test_opts())
            .await
            .expect("404");
        assert_eq!(missing.status, 404);
        server.shutdown();
    }

    #[test]
    fn tool08_ip_classification() {
        let public = ["8.8.8.8", "1.1.1.1"];
        for ip in public {
            assert!(ip_is_public(ip.parse().expect("ip")), "{ip}");
        }
        assert!(ip_is_public("2606:4700:4700::1111".parse().expect("ip6")));
        let private = [
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "127.0.0.1",
            "169.254.1.1",
            "0.0.0.0",
            "224.0.0.1",
            "100.64.0.1",
            "192.0.2.1",
            "::1",
            "fe80::1",
            "::ffff:10.0.0.1",
            "::ffff:8.8.8.8",
        ];
        for ip in private {
            if ip == "::ffff:8.8.8.8" {
                // Mapped public stays public.
                assert!(ip_is_public(ip.parse().expect("ip")), "{ip}");
            } else {
                assert!(!ip_is_public(ip.parse().expect("ip")), "{ip}");
            }
        }
        assert!(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)).is_loopback());
    }

    #[tokio::test]
    async fn tool08_loopback_private_redirect_guards() {
        // Loopback refused without the explicit test exception.
        let denied = fetch("http://127.0.0.1:9/", None, FetchOptions::default()).await;
        assert_eq!(denied, Err(FetchError::PrivateHost));
        // Literal private address refused before any dial.
        assert_eq!(
            fetch("http://10.0.0.1/", None, FetchOptions::default()).await,
            Err(FetchError::PrivateHost)
        );
        // Redirect to a private literal is refused even with loopback allowed.
        let server = TestServer::spawn().await;
        let err = fetch(&format!("{}/go-private", server.base), None, test_opts()).await;
        assert_eq!(err, Err(FetchError::PrivateHost));
        server.shutdown();
    }

    #[tokio::test]
    async fn tool08_large_body_deadline_auth() {
        let server = TestServer::spawn().await;
        let big = fetch(&format!("{}/big", server.base), None, test_opts())
            .await
            .expect("big");
        assert!(big.truncated);
        assert_eq!(big.text.len(), super::BODY_CAP_BYTES);

        let mut opts = test_opts();
        opts.timeout = Duration::from_millis(300);
        assert_eq!(
            fetch(&format!("{}/slow", server.base), None, opts).await,
            Err(FetchError::Deadline)
        );

        // Bearer reaches the first hop only, never the redirect target.
        fetch(&format!("{}/login", server.base), Some("tok"), test_opts())
            .await
            .expect("login");
        let seen = server.seen.lock().expect("seen").clone();
        let login = seen
            .iter()
            .find(|s| s.path == "/login")
            .expect("login seen");
        let check = seen
            .iter()
            .find(|s| s.path == "/check")
            .expect("check seen");
        assert_eq!(login.auth.as_deref(), Some("Bearer tok"));
        assert_eq!(check.auth, None);
        server.shutdown();
    }

    #[tokio::test]
    async fn tool08_localhost_needs_exception() {
        assert_eq!(
            check_host("localhost", 80, false).await,
            Err(FetchError::PrivateHost)
        );
    }

    #[test]
    fn units_extract_and_html() {
        assert_eq!(extract_text(b"plain", Some("text/plain")), "plain");
        assert_eq!(html_to_text("<p>a</p><p>b</p>"), "a\nb");
    }
}
