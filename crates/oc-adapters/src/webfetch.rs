//! Webfetch tool for T11 (TOOL07–TOOL08), hardened for T38 AUD25/AUD26.
//!
//! Plain `GET` with metadata-first results (`status`, `content-type`,
//! original URL), bounded readable text extraction (HTML/JSON/text), manual
//! redirect handling through standard URL joining (per-hop SSRF re-check,
//! auth never inherited) and dial-bound egress control: a
//! `reqwest::dns::Resolve` wrapper rejects non-public addresses at connect
//! time, so a DNS answer that changes between the pre-dial check and the
//! actual connection cannot reach a private endpoint. One total deadline
//! spans DNS, every redirect hop and the body. Loopback and private ranges
//! are refused unless the explicit test-only `allow_loopback` exception is
//! set; production callers leave it `false`.

use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Url;
use thiserror::Error;

/// Response body cap (bytes retained; overflow flagged, never grown).
pub const BODY_CAP_BYTES: usize = 1024 * 1024;
/// Max redirect hops followed.
pub const MAX_REDIRECTS: usize = 5;

/// Error type used by resolver futures; same shape as reqwest's internal
/// alias, which is not publicly nameable in 0.13.
type DnsBoxError = Box<dyn std::error::Error + Send + Sync>;

/// Typed fetch errors (no body contents, no credentials).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Malformed URL or unsupported scheme.
    #[error("invalid url")]
    InvalidUrl,
    /// SSRF guard: non-public dial target.
    #[error("private host refused")]
    PrivateHost,
    /// Hostname resolution failed or returned no addresses.
    #[error("DNS resolution failed")]
    Dns,
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
    /// Total deadline per call (DNS, all hops, body).
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

/// Egress policy for a single address.
fn ensure_public(ip: IpAddr, allow_loopback: bool) -> Result<(), FetchError> {
    if (allow_loopback && ip.is_loopback()) || ip_is_public(ip) {
        Ok(())
    } else {
        Err(FetchError::PrivateHost)
    }
}

/// Pre-dial guard: every resolved address of `host:port` must be public
/// (or loopback under the explicit test exception). Used by callers that
/// dial outside `fetch` (MCP remote).
pub async fn check_host(host: &str, port: u16, allow_loopback: bool) -> Result<(), FetchError> {
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| FetchError::Dns)?;
    let mut any = false;
    for addr in addrs {
        any = true;
        ensure_public(addr.ip(), allow_loopback)?;
    }
    if any { Ok(()) } else { Err(FetchError::Dns) }
}

/// Parse and validate a request URL: http/https only, host required,
/// userinfo refused, fragment stripped (never sent).
fn parse_request_url(url: &str) -> Result<Url, FetchError> {
    let mut url = Url::parse(url).map_err(|_| FetchError::InvalidUrl)?;
    validate_url(&mut url)?;
    Ok(url)
}

/// Validate scheme/host/userinfo of `url` and strip its fragment.
fn validate_url(url: &mut Url) -> Result<(), FetchError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(FetchError::InvalidUrl),
    }
    if url.host_str().is_none() {
        return Err(FetchError::InvalidUrl);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::InvalidUrl);
    }
    url.set_fragment(None);
    Ok(())
}

/// Resolve a `Location` value against the current hop's URL. Standard
/// joining handles relative, protocol-relative, query-only, fragment-only
/// and absolute targets; every result is re-validated.
fn redirect_target(base: &Url, location: &str) -> Result<Url, FetchError> {
    let location = location.trim();
    if location.is_empty() || location.contains('\0') {
        return Err(FetchError::BadResponse);
    }
    let mut target = base.join(location).map_err(|_| FetchError::BadResponse)?;
    validate_url(&mut target).map_err(|_| FetchError::BadResponse)?;
    Ok(target)
}

/// Marker error for an egress-blocked DNS answer. Survives reqwest's error
/// wrapping, so `fetch` can report `PrivateHost` for refused dials.
#[derive(Debug)]
struct BlockedAddress;

impl std::fmt::Display for BlockedAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("non-public address refused")
    }
}

impl std::error::Error for BlockedAddress {}

/// System DNS lookup through tokio's resolver (never recurses into reqwest).
struct SystemResolver;

impl reqwest::dns::Resolve for SystemResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((host, 0))
                .await
                .map_err(|e| -> DnsBoxError { Box::new(e) })?;
            Ok(Box::new(addrs) as reqwest::dns::Addrs)
        })
    }
}

/// Dial-bound egress guard: resolves through `inner` and rejects the whole
/// answer when any address is non-public (loopback only under the explicit
/// test flag). Installed as the client's `dns_resolver`, so it runs at
/// connect time and closes the rebinding window between pre-check and dial.
struct GuardedResolver {
    inner: Arc<dyn reqwest::dns::Resolve>,
    allow_loopback: bool,
}

impl GuardedResolver {
    fn new(inner: Arc<dyn reqwest::dns::Resolve>, allow_loopback: bool) -> Self {
        Self {
            inner,
            allow_loopback,
        }
    }
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let inner = self.inner.clone();
        let allow_loopback = self.allow_loopback;
        Box::pin(async move {
            let addrs = inner.resolve(name).await?;
            let mut resolved = Vec::new();
            for addr in addrs {
                if ensure_public(addr.ip(), allow_loopback).is_err() {
                    return Err(Box::new(BlockedAddress) as DnsBoxError);
                }
                resolved.push(addr);
            }
            if resolved.is_empty() {
                return Err(Box::new(BlockedAddress) as DnsBoxError);
            }
            Ok(Box::new(resolved.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Per-hop pre-dial check through the guarded resolver. IP literals never
/// reach the resolver (hyper dials them directly), so they are classified
/// here.
async fn check_dial_target(
    resolver: &Arc<dyn reqwest::dns::Resolve>,
    host: &str,
    allow_loopback: bool,
) -> Result<(), FetchError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return ensure_public(ip, allow_loopback);
    }
    let name = host
        .parse::<reqwest::dns::Name>()
        .map_err(|_| FetchError::InvalidUrl)?;
    let addrs = resolver
        .resolve(name)
        .await
        .map_err(|_| FetchError::PrivateHost)?;
    let mut any = false;
    for addr in addrs {
        any = true;
        ensure_public(addr.ip(), allow_loopback)?;
    }
    if any {
        Ok(())
    } else {
        Err(FetchError::PrivateHost)
    }
}

/// Remaining slice of the total budget; exhaustion is a deadline.
fn budget_left(deadline: Instant) -> Result<Duration, FetchError> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(FetchError::Deadline)
    } else {
        Ok(left)
    }
}

/// True when `err` wraps the egress guard's `BlockedAddress`.
fn blocked_address(err: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(current) = source {
        if current.downcast_ref::<BlockedAddress>().is_some() {
            return true;
        }
        source = current.source();
    }
    false
}

/// Fetch a URL with full guard rails.
///
/// `auth` (if any) is sent only on the first hop, never inherited by
/// redirects. One total deadline spans DNS, every hop and the body; bodies
/// stream with a byte cap, HTML is reduced to readable text, JSON is
/// pretty-printed when parseable.
pub async fn fetch(
    url: &str,
    auth: Option<&str>,
    opts: FetchOptions,
) -> Result<FetchResult, FetchError> {
    let lookup: Arc<dyn reqwest::dns::Resolve> = Arc::new(SystemResolver);
    fetch_with_resolver(url, auth, opts, lookup).await
}

/// `fetch` with an injectable lookup resolver (tests simulate rebinding
/// answers). The resolver is wrapped by the dial-time egress guard.
async fn fetch_with_resolver(
    url: &str,
    auth: Option<&str>,
    opts: FetchOptions,
    lookup: Arc<dyn reqwest::dns::Resolve>,
) -> Result<FetchResult, FetchError> {
    if auth.map(str::is_empty).unwrap_or(false) {
        return Err(FetchError::InvalidUrl);
    }
    let mut current = parse_request_url(url)?;
    let deadline = Instant::now() + opts.timeout;
    let resolver: Arc<dyn reqwest::dns::Resolve> =
        Arc::new(GuardedResolver::new(lookup, opts.allow_loopback));
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent(crate::WEB_USER_AGENT)
        .connect_timeout(opts.connect_timeout)
        .dns_resolver(resolver.clone())
        .build()
        .map_err(|_| FetchError::Transport)?;

    let mut hops = 0usize;
    loop {
        // `host_str()` keeps IPv6 brackets; the guard classifies the literal.
        let host = {
            let raw = current.host_str().ok_or(FetchError::InvalidUrl)?;
            raw.strip_prefix('[')
                .and_then(|h| h.strip_suffix(']'))
                .unwrap_or(raw)
                .to_string()
        };
        check_dial_target(&resolver, &host, opts.allow_loopback).await?;

        let mut req = client.get(current.clone()).timeout(budget_left(deadline)?);
        if hops == 0
            && let Some(token) = auth
        {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.map_err(|e| {
            if blocked_address(&e) {
                FetchError::PrivateHost
            } else if e.is_timeout() || e.is_connect() {
                FetchError::Deadline
            } else {
                FetchError::Transport
            }
        })?;
        // Post-dial rebinding guard, kept as defence in depth.
        if let Some(peer) = resp.remote_addr()
            && ensure_public(peer.ip(), opts.allow_loopback).is_err()
        {
            return Err(FetchError::PrivateHost);
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
            current = redirect_target(&current, location)?;
            hops += 1;
            continue;
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.split(';').next().unwrap_or("").trim().to_lowercase())
            .filter(|ct| !ct.is_empty());
        let body_budget = budget_left(deadline)?;
        let body = tokio::time::timeout(body_budget, read_capped_body(resp, opts.body_cap))
            .await
            .map_err(|_| FetchError::Deadline)??;
        let (text, truncated) = (
            extract_text(&body.body, content_type.as_deref()),
            body.truncated,
        );
        return Ok(FetchResult {
            status,
            content_type,
            url: url.to_string(),
            final_url: current.to_string(),
            text,
            truncated,
        });
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
///
/// Scanning only slices the input at ASCII delimiter boundaries (`<`, `>`,
/// `;`), so multi-byte UTF-8 text is preserved exactly and can never panic.
pub fn html_to_text(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut skip: Option<&'static str> = None;
    let mut i = 0usize;
    let mut text_start = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if skip.is_none() && text_start < i {
            out.push_str(&decode_entities(&html[text_start..i]));
        }
        if bytes[i..].starts_with(b"<!--") {
            i = match find_sequence(&bytes[i + 4..], b"-->") {
                Some(offset) => i + 4 + offset + 3,
                None => bytes.len(),
            };
            text_start = i;
            continue;
        }
        match bytes[i..].iter().position(|b| *b == b'>') {
            Some(offset) => {
                let close_at = i + offset;
                let name = tag_name(&html[i + 1..close_at]);
                if let Some(open) = skip {
                    if name.strip_prefix('/') == Some(open) {
                        skip = None;
                    }
                } else if name == "script" || name == "style" {
                    skip = Some(if name == "script" { "script" } else { "style" });
                } else if is_block_tag(&name) {
                    out.push('\n');
                }
                i = close_at + 1;
            }
            None => i = bytes.len(),
        }
        text_start = i;
    }
    if skip.is_none() && text_start < bytes.len() {
        out.push_str(&decode_entities(&html[text_start..]));
    }
    collapse_whitespace(&out)
}

fn find_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn tag_name(fragment: &str) -> String {
    let inner: String = fragment
        .chars()
        .take_while(|c| *c != '>' && !c.is_whitespace())
        .collect();
    // Keep a possible leading `/` so closing tags compare: "/style".
    let inner = inner.trim_end_matches('/').to_string();
    inner.to_lowercase()
}

fn is_block_tag(name: &str) -> bool {
    matches!(name, "p" | "br" | "div" | "li" | "tr") || name.starts_with('h')
}

/// Decode named/numeric entities in one text segment. Unknown or malformed
/// entities stay literal.
fn decode_entities(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        let candidate = &rest[index..];
        match decode_entity(candidate) {
            Some((decoded, consumed)) => {
                out.push(decoded);
                rest = &candidate[consumed..];
            }
            None => {
                out.push('&');
                rest = &candidate[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Decode one `&...;` sequence; returns the character and bytes consumed.
fn decode_entity(candidate: &str) -> Option<(char, usize)> {
    let end = candidate.find(';')?;
    let body = candidate.get(1..end)?;
    if body.is_empty() || body.len() > 10 {
        return None;
    }
    let decoded = if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        char::from_u32(u32::from_str_radix(hex, 16).ok()?)?
    } else if let Some(decimal) = body.strip_prefix('#') {
        char::from_u32(decimal.parse::<u32>().ok()?)?
    } else {
        match body {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => ' ',
            "mdash" => '\u{2014}',
            "ndash" => '\u{2013}',
            "hellip" => '\u{2026}',
            _ => return None,
        }
    };
    Some((decoded, end + 1))
}

fn collapse_whitespace(text: &str) -> String {
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
        FetchError, FetchOptions, check_host, extract_text, fetch, fetch_with_resolver,
        html_to_text, ip_is_public,
    };
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Recorded inbound request (path + headers of interest).
    #[derive(Debug, Clone)]
    struct Seen {
        path: String,
        auth: Option<String>,
        user_agent: Option<String>,
    }

    struct TestServer {
        base: String,
        addr: SocketAddr,
        seen: Arc<Mutex<Vec<Seen>>>,
        handle: tokio::task::JoinHandle<()>,
    }

    impl TestServer {
        async fn spawn() -> Self {
            Self::spawn_on("127.0.0.1:0").await
        }

        async fn spawn_on(bind: &str) -> Self {
            let listener = tokio::net::TcpListener::bind(bind).await.expect("bind");
            Self::serve(listener)
        }

        async fn try_spawn_on(bind: &str) -> Option<Self> {
            let listener = tokio::net::TcpListener::bind(bind).await.ok()?;
            Some(Self::serve(listener))
        }

        fn serve(listener: tokio::net::TcpListener) -> Self {
            let addr = listener.local_addr().expect("addr");
            let base = format!("http://{addr}");
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
                            user_agent: headers.get("user-agent").cloned(),
                        });
                        let host = headers.get("host").cloned().unwrap_or_default();
                        let route_path = path.split('?').next().unwrap_or("/");
                        let (status, extra, body, delay_ms) = route(route_path, &host);
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
            Self {
                base,
                addr,
                seen,
                handle,
            }
        }

        fn port(&self) -> u16 {
            self.addr.port()
        }

        fn shutdown(self) {
            self.handle.abort();
        }
    }

    /// Test routes: (status line, extra headers, body, delay ms).
    fn route(path: &str, host: &str) -> (&'static str, Vec<(&'static str, String)>, Vec<u8>, u64) {
        match path {
            "/text" => (
                "200 OK",
                vec![("Content-Type", "text/plain".to_string())],
                b"hello world".to_vec(),
                0,
            ),
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
            "/redir" => (
                "302 Found",
                vec![("Location", "/text".to_string())],
                Vec::new(),
                0,
            ),
            "/rel" => (
                "302 Found",
                vec![("Location", "text".to_string())],
                Vec::new(),
                0,
            ),
            "/proto" => (
                "302 Found",
                vec![("Location", format!("//{host}/text"))],
                Vec::new(),
                0,
            ),
            "/to-slow" => (
                "302 Found",
                vec![("Location", "/slow".to_string())],
                Vec::new(),
                0,
            ),
            "/go-private" => (
                "302 Found",
                vec![("Location", "http://10.0.0.1/".to_string())],
                Vec::new(),
                0,
            ),
            "/big" => (
                "200 OK",
                vec![("Content-Type", "text/plain".to_string())],
                vec![b'x'; 3 * 1024 * 1024],
                0,
            ),
            "/slow" => (
                "200 OK",
                vec![("Content-Type", "text/plain".to_string())],
                b"late".to_vec(),
                5000,
            ),
            "/login" => (
                "302 Found",
                vec![("Location", "/check".to_string())],
                Vec::new(),
                0,
            ),
            "/check" => (
                "200 OK",
                vec![("Content-Type", "text/plain".to_string())],
                b"checked".to_vec(),
                0,
            ),
            _ => (
                "404 Not Found",
                vec![("Content-Type", "text/plain".to_string())],
                b"nope".to_vec(),
                0,
            ),
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

    /// Resolver mapping every name to a fixed answer (test injection point).
    struct FixedResolver {
        addrs: Vec<SocketAddr>,
    }

    impl FixedResolver {
        fn new(addrs: Vec<SocketAddr>) -> Self {
            Self { addrs }
        }
    }

    impl reqwest::dns::Resolve for FixedResolver {
        fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
            ready_addrs(self.addrs.clone())
        }
    }

    /// Resolver whose first answer is `first`, every later answer `later`.
    struct FlipResolver {
        calls: AtomicUsize,
        first: Vec<SocketAddr>,
        later: Vec<SocketAddr>,
    }

    impl FlipResolver {
        fn new(first: Vec<SocketAddr>, later: Vec<SocketAddr>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                first,
                later,
            }
        }
    }

    impl reqwest::dns::Resolve for FlipResolver {
        fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                ready_addrs(self.first.clone())
            } else {
                ready_addrs(self.later.clone())
            }
        }
    }

    fn ready_addrs(addrs: Vec<SocketAddr>) -> reqwest::dns::Resolving {
        let answer: Result<reqwest::dns::Addrs, super::DnsBoxError> =
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs);
        Box::pin(std::future::ready(answer))
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
        assert_eq!(
            login.user_agent.as_deref(),
            Some(crate::WEB_USER_AGENT),
            "page fetches carry the browser-like identity"
        );
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

    #[test]
    fn aud25_html_unicode_entities_and_malformed() {
        let unicode = html_to_text("<p>Привет 🦀</p>");
        assert!(unicode.contains("Привет 🦀"), "got: {unicode}");

        let entities = html_to_text(
            "a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos; f&nbsp;g &mdash; &ndash; &hellip; &#39; &#x1F980;",
        );
        assert!(
            entities.contains("a & b <c> \"d\" 'e' f g — – … ' 🦀"),
            "got: {entities}"
        );

        let unknown = html_to_text("keep &bogus; and &#xZZ; literal");
        assert!(unknown.contains("&bogus;"), "got: {unknown}");
        assert!(unknown.contains("&#xZZ;"), "got: {unknown}");

        // Unterminated comment and tag: no panic, tail dropped.
        let malformed = html_to_text("<p>start <b>bold <i>unclosed<!-- tail");
        assert!(!malformed.contains("tail"), "got: {malformed}");
        assert!(malformed.contains("start"), "got: {malformed}");
        let unclosed_tag = html_to_text("<p>visible<broken");
        assert!(unclosed_tag.contains("visible"), "got: {unclosed_tag}");

        // Case-insensitive script/style skipping.
        let upper = html_to_text("<SCRIPT>var x=1;</SCRIPT><STYLE>.y{}</STYLE><P>ok</P>");
        assert_eq!(upper, "ok");

        // Multi-byte characters adjacent to delimiters stay intact.
        assert_eq!(html_to_text("é<é>é"), "éé");
    }

    #[tokio::test]
    async fn aud25_url_relative_query_ipv6() {
        let server = TestServer::spawn().await;

        // Query-only URL: path and query reach the server and the final URL.
        let query_url = format!("{}/text?x=1", server.base);
        let query = fetch(&query_url, None, test_opts()).await.expect("query");
        assert_eq!(query.status, 200);
        assert_eq!(query.final_url, query_url);
        {
            let seen = server.seen.lock().expect("seen");
            assert!(seen.iter().any(|s| s.path == "/text?x=1"), "seen: {seen:?}");
        }

        // Relative redirect (`Location: text`).
        let rel = fetch(&format!("{}/rel", server.base), None, test_opts())
            .await
            .expect("relative");
        assert_eq!(rel.text, "hello world");
        assert_eq!(rel.final_url, format!("{}/text", server.base));

        // Protocol-relative redirect.
        let proto = fetch(&format!("{}/proto", server.base), None, test_opts())
            .await
            .expect("protocol-relative");
        assert_eq!(proto.text, "hello world");
        assert_eq!(proto.final_url, format!("{}/text", server.base));

        // Fragment stripped from the request and from the final URL.
        let frag = fetch(&format!("{}/text#section", server.base), None, test_opts())
            .await
            .expect("fragment");
        assert_eq!(frag.final_url, format!("{}/text", server.base));
        {
            let seen = server.seen.lock().expect("seen");
            assert!(seen.iter().all(|s| !s.path.contains('#')), "seen: {seen:?}");
        }

        // Userinfo is refused before any dial.
        assert_eq!(
            fetch(
                "http://user:pass@example.com/",
                None,
                FetchOptions::default()
            )
            .await,
            Err(FetchError::InvalidUrl)
        );

        // IPv6 literal through the loopback exception.
        match TestServer::try_spawn_on("[::1]:0").await {
            Some(v6) => {
                let result = fetch(&format!("{}/text", v6.base), None, test_opts())
                    .await
                    .expect("ipv6");
                assert_eq!(result.text, "hello world");
                v6.shutdown();
            }
            None => {
                // No AF_INET6 in this environment: still prove the literal is
                // accepted by URL parsing and the guard, failing only later at
                // connect time.
                let err = fetch("http://[::1]:9/text", None, test_opts()).await;
                assert_eq!(err, Err(FetchError::Deadline), "unexpected: {err:?}");
            }
        }
        server.shutdown();
    }

    #[tokio::test]
    async fn aud26_dial_bound_private_answer_sends_no_request() {
        let server = TestServer::spawn().await;
        let blocked: SocketAddr = format!("127.0.0.1:{}", server.port())
            .parse()
            .expect("addr");
        let mut opts = test_opts();
        opts.allow_loopback = false;

        // Fake name that the injected resolver maps to loopback: the dial-time
        // guard refuses it and the server records nothing.
        let fake: Arc<dyn reqwest::dns::Resolve> = Arc::new(FixedResolver::new(vec![blocked]));
        let err = fetch_with_resolver("http://blocked.invalid/text", None, opts, fake).await;
        assert_eq!(err, Err(FetchError::PrivateHost));
        assert!(
            server.seen.lock().expect("seen").is_empty(),
            "private answer must not produce a request"
        );

        // Rebinding flip: public first (pre-check passes), loopback at dial.
        let flip: Arc<dyn reqwest::dns::Resolve> = Arc::new(FlipResolver::new(
            vec!["8.8.8.8:80".parse().expect("public")],
            vec![blocked],
        ));
        let err = fetch_with_resolver("http://flip.invalid/text", None, opts, flip).await;
        assert_eq!(
            err,
            Err(FetchError::PrivateHost),
            "flip must be refused at dial time"
        );
        assert!(
            server.seen.lock().expect("seen").is_empty(),
            "flipped answer must not produce a request"
        );
        server.shutdown();
    }

    #[tokio::test]
    async fn aud26_total_deadline_spans_redirects() {
        let server = TestServer::spawn().await;
        let mut opts = test_opts();
        opts.timeout = Duration::from_millis(400);
        let started = std::time::Instant::now();
        let result = fetch(&format!("{}/to-slow", server.base), None, opts).await;
        let elapsed = started.elapsed();
        assert_eq!(result, Err(FetchError::Deadline));
        assert!(
            elapsed < Duration::from_secs(2),
            "total budget must bound the slow hop, elapsed {elapsed:?}"
        );
        server.shutdown();
    }
}
