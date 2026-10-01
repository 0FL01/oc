use super::*;
use std::sync::atomic::AtomicUsize;

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
pub(super) static JOINS: AtomicUsize = AtomicUsize::new(0);
pub(super) struct Worker;
impl Worker {
    pub(super) fn enter() -> Self {
        ACTIVE.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

const HTML: &str = "<html><head><script>HIDDEN</script></head><body><h1>Привет &amp; 🦀</h1><ul><li>one</li><li>二</li></ul><p><a href='/docs'>guide</a> <code>x &lt; y</code></p><pre><code class='language-rust'>fn main() {\n  println!(\"é\");\n}</code></pre><table><tr><th>Name</th><th>Value</th></tr><tr><td>é</td><td>42</td></tr></table></body></html>";

#[test]
fn tool17_unicode_structure_and_honest_mime() {
    let token = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(2);
    let convert = |bytes: &[u8], mime, format| {
        render::convert(bytes, mime, format, deadline, &token)
            .unwrap()
            .0
    };
    let markdown = convert(HTML.as_bytes(), Some("text/html"), FetchFormat::Markdown);
    for expected in [
        "# Привет & 🦀",
        "- one",
        "- 二",
        "[guide](/docs)",
        "`x < y`",
        "```rust\nfn main()",
        "| Name | Value |",
        "| --- | --- |",
        "| é | 42 |",
    ] {
        assert!(
            markdown.contains(expected),
            "missing {expected:?}: {markdown}"
        );
    }
    assert!(!markdown.contains("HIDDEN"));
    let text = convert(HTML.as_bytes(), Some("text/html"), FetchFormat::Text);
    assert!(
        text.contains("Привет & 🦀")
            && text.contains("guide")
            && !text.contains("HIDDEN")
            && !text.contains("<h1>")
    );
    assert_eq!(
        convert(HTML.as_bytes(), Some("text/html"), FetchFormat::Html),
        HTML
    );
    for mime in ["text/plain", "application/json"] {
        for format in [FetchFormat::Html, FetchFormat::Markdown, FetchFormat::Text] {
            assert_eq!(
                convert(b"{\"text\":\"<p>literal</p>\"}", Some(mime), format),
                "{\"text\":\"<p>literal</p>\"}"
            );
        }
    }
}

#[test]
fn tool17_parser_caps_and_deadline_refuse_partial_success() {
    let token = AtomicBool::new(false);
    assert_eq!(
        render::convert(
            HTML.as_bytes(),
            Some("text/html"),
            FetchFormat::Markdown,
            Instant::now(),
            &token
        ),
        Err(FetchError::Deadline)
    );
    token.store(true, Ordering::Release);
    assert_eq!(
        render::convert(
            HTML.as_bytes(),
            Some("text/html"),
            FetchFormat::Markdown,
            Instant::now() + Duration::from_secs(1),
            &token
        ),
        Err(FetchError::Cancelled)
    );
    token.store(false, Ordering::Release);
    let huge = "🦀".repeat(BODY_CAP_BYTES);
    let (output, truncated) = render::convert(
        huge.as_bytes(),
        Some("text/plain"),
        FetchFormat::Markdown,
        Instant::now() + Duration::from_secs(2),
        &token,
    )
    .unwrap();
    assert!(truncated && output.len() <= CONTENT_CAP_BYTES && output.ends_with('🦀'));
    let deep = "<div>".repeat(1025);
    assert_eq!(
        render::convert(
            deep.as_bytes(),
            Some("text/html"),
            FetchFormat::Markdown,
            Instant::now() + Duration::from_secs(2),
            &token
        ),
        Err(FetchError::TooLarge)
    );
}

struct HeldDns;
impl reqwest::dns::Resolve for HeldDns {
    fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(std::future::pending())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn tool17_dns_is_inside_total_deadline() {
    let opts = FetchOptions {
        timeout: Duration::from_millis(40),
        ..FetchOptions::default()
    };
    let started = Instant::now();
    assert_eq!(
        fetch_with_resolver("http://held.invalid/", None, opts, Arc::new(HeldDns)).await,
        Err(FetchError::Deadline)
    );
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[tokio::test(flavor = "current_thread")]
async fn tool17_held_body_deadline_cancel_and_conversion_join_keep_executor_responsive() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for mode in ["deadline", "cancel", "conversion"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let (release, hold) = tokio::sync::oneshot::channel::<()>();
        let (request, seen) = tokio::sync::oneshot::channel::<()>();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let bytes = socket.read(&mut buffer).await.unwrap();
            assert!(
                bytes > 0
                    && std::str::from_utf8(&buffer[..bytes])
                        .unwrap()
                        .starts_with("GET ")
            );
            request.send(()).unwrap();
            if mode == "conversion" {
                let body = "<p>é &amp; 🦀</p>".repeat(40000);
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                socket.write_all(body.as_bytes()).await.unwrap();
            } else {
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nheld",
                    )
                    .await
                    .unwrap();
            }
            let _ = hold.await;
        });
        let cancel = AtomicBool::new(false);
        let opts = FetchOptions {
            allow_loopback: true,
            timeout: Duration::from_millis(if mode == "deadline" { 80 } else { 2000 }),
            ..FetchOptions::default()
        };
        let before = JOINS.load(Ordering::SeqCst);
        let ticks = AtomicUsize::new(0);
        let steering = async {
            seen.await.unwrap();
            if mode == "conversion" {
                tokio::time::timeout(Duration::from_secs(1), async {
                    while ACTIVE.load(Ordering::SeqCst) == 0 {
                        ticks.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                })
                .await
                .unwrap();
            } else {
                tokio::time::sleep(Duration::from_millis(20)).await;
                ticks.fetch_add(1, Ordering::SeqCst);
            }
            if mode != "deadline" {
                cancel.store(true, Ordering::Release);
            }
        };
        let started = Instant::now();
        let (result, ()) = tokio::join!(
            fetch_formatted(&url, opts, FetchFormat::Markdown, &cancel),
            steering
        );
        assert_eq!(
            result,
            Err(if mode == "deadline" {
                FetchError::Deadline
            } else {
                FetchError::Cancelled
            })
        );
        assert!(started.elapsed() < Duration::from_millis(1500));
        assert!(ticks.load(Ordering::SeqCst) > 0, "async heartbeat must run");
        assert_eq!(ACTIVE.load(Ordering::SeqCst), 0, "no orphan conversion");
        assert_eq!(
            JOINS.load(Ordering::SeqCst) - before,
            usize::from(mode == "conversion")
        );
        release.send(()).unwrap();
        peer.await.unwrap();
    }
}
