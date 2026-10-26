use super::*;
use crate::config::{ProviderEntry, ProviderTimeout, provider_wire};
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

#[tokio::test]
async fn cfg16_pre_send_network_and_deadline_reach_retry_without_losing_authority_errors() {
    let cancel = AtomicBool::new(false);
    for transport in [TransportKind::Network, TransportKind::Deadline] {
        let operation = async {
            if transport == TransportKind::Deadline {
                std::future::pending::<()>().await;
            }
            Err::<(), _>(ProviderError::Transport)
        };
        let error = pre_send(
            tokio::time::Instant::now(),
            &cancel,
            "DNS resolution before send",
            operation,
        )
        .await
        .unwrap_err();
        let ProviderError::Request(failure) = error else {
            panic!("pre-send failure must reach retry policy");
        };
        assert_eq!(failure.kind, FailureKind::Transport);
        assert_eq!(failure.transport, Some(transport));
        assert_eq!(failure.operation, Operation::Request);
        assert_eq!(failure.delivery, Delivery::NotSent);
        assert!(!failure.output_committed);
        assert!(failure.http_status.is_none());
        assert!(failure.retry_eligible());
        assert!(failure.to_string().contains("DNS resolution before send"));
    }
    let refused = pre_send(
        tokio::time::Instant::now(),
        &cancel,
        "DNS resolution before send",
        std::future::ready(Err::<(), _>(ProviderError::PrivateHost)),
    )
    .await;
    assert!(matches!(refused, Err(ProviderError::PrivateHost)));
    cancel.store(true, Ordering::Relaxed);
    let cancelled = pre_send(
        tokio::time::Instant::now(),
        &cancel,
        "DNS resolution before send",
        std::future::pending::<Result<(), ProviderError>>(),
    )
    .await;
    assert!(matches!(cancelled, Err(ProviderError::Cancelled)));
}

async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0 && bytes.len() + n <= 64 * 1024);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return;
            }
        }
    }
}

#[tokio::test]
async fn cfg16_http_headers_and_chunks_have_independent_optional_deadlines() {
    for (delay_headers, header, chunk, expected) in [
        (
            true,
            Some(100),
            None,
            Some((Operation::Request, TransportKind::Deadline)),
        ),
        (true, None, Some(100), None),
        (false, Some(100), None, None),
        (
            false,
            None,
            Some(100),
            Some((Operation::Read, TransportKind::IdleTimeout)),
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            if delay_headers {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            if socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"started\"}\n\n").await.is_err() { return; }
            if !delay_headers {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            let _ = socket.write_all(b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n").await;
        });
        let config = ResponsesConfig {
            base_url: format!("http://{address}/v1"),
            api_key: "synthetic".into(),
            timeout: None,
            header_timeout_ms: header,
            chunk_timeout_ms: chunk,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: Default::default(),
            cache: CacheMode::None,
            wire: Default::default(),
        };
        let mut observed = false;
        let mut attempts = 0;
        let result = stream_input_counted(
            &config,
            "wire",
            None,
            &[InputItem::message(InputRole::User, "hello")],
            &[],
            100,
            &AtomicBool::new(false),
            &mut |item| observed |= matches!(item, StreamItem::TextDelta(_)),
            &mut || {
                attempts += 1;
                std::future::ready(Ok(()))
            },
        )
        .await;
        if let Some((operation, transport)) = expected {
            let ProviderError::Request(failure) = result.unwrap_err() else {
                panic!("expected typed timeout");
            };
            assert_eq!(failure.operation, operation);
            assert_eq!(failure.transport, Some(transport));
            assert_eq!(failure.output_committed, !delay_headers);
            assert_eq!(observed, !delay_headers);
        } else {
            result.unwrap();
            assert!(observed);
        }
        assert_eq!(attempts, 1);
        peer.await.unwrap();
    }
}

#[tokio::test]
async fn cfg16_disabled_http_deadlines_still_cancel_headers_and_body() {
    for headers in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (ready, waiting) = tokio::sync::oneshot::channel();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            if headers {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"started\"}\n\n").await.unwrap();
            }
            ready.send(()).unwrap();
            assert_eq!(
                socket.read(&mut [0; 1]).await.unwrap(),
                0,
                "cancel closes the owned request"
            );
        });
        let config = ResponsesConfig {
            base_url: format!("http://{address}/v1"),
            api_key: "synthetic".into(),
            timeout: Some(false),
            header_timeout_ms: None,
            chunk_timeout_ms: None,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: Default::default(),
            cache: CacheMode::None,
            wire: Default::default(),
        };
        let cancel = AtomicBool::new(false);
        let mut observed = false;
        let input = [InputItem::message(InputRole::User, "hello")];
        let mut observe = |item: &StreamItem| observed |= matches!(item, StreamItem::TextDelta(_));
        let mut dispatch = || std::future::ready(Ok(()));
        let request = stream_input_counted(
            &config,
            "wire",
            None,
            &input,
            &[],
            100,
            &cancel,
            &mut observe,
            &mut dispatch,
        );
        let signal = async {
            waiting.await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel.store(true, Ordering::Relaxed);
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(request, signal)
        })
        .await
        .unwrap();
        assert_eq!(result, Err(ProviderError::Cancelled));
        assert_eq!(observed, headers);
        tokio::time::timeout(Duration::from_secs(2), peer)
            .await
            .unwrap()
            .unwrap();
    }
}

#[test]
fn go03_numeric_timeout_admission_and_units_are_exact() {
    for field in ["timeout", "headerTimeout", "chunkTimeout"] {
        for value in [json!(false), json!(1234)] {
            let entry: ProviderEntry =
                serde_json::from_value(json!({"options":{(field):value}})).unwrap();
            let wire = provider_wire("fixture", &entry).unwrap();
            if field == "timeout" {
                assert_eq!(wire.total_timeout_ms, value.as_u64());
            }
            assert_eq!(serde_json::to_value(&entry.options).unwrap()[field], value);
        }
        for value in [json!(true), json!(0)] {
            let entry: ProviderEntry =
                serde_json::from_value(json!({"options":{(field):value}})).unwrap();
            assert!(provider_wire("fixture", &entry).is_err());
        }
        for value in [json!(-1), json!(1.5), json!("1234")] {
            assert!(
                serde_json::from_value::<ProviderEntry>(json!({"options":{(field):value}}))
                    .is_err()
            );
        }
    }
    assert_eq!(ProviderTimeout::Flag(false).legacy_flag(), Some(false));
    assert_eq!(ProviderTimeout::Milliseconds(1234).legacy_flag(), None);
}

#[tokio::test]
async fn go03_absent_and_false_timeout_have_no_total_deadline() {
    for timeout in [None, Some(false)] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            tokio::time::sleep(Duration::from_millis(250)).await;
            let body = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n";
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let config = ResponsesConfig {
            base_url: format!("http://{address}/prefix"),
            api_key: "synthetic".into(),
            timeout,
            header_timeout_ms: Some(crate::provider::HEADER_TIMEOUT_MS),
            chunk_timeout_ms: Some(1000),
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: Default::default(),
            cache: crate::provider::CacheMode::None,
            wire: Default::default(),
        };
        stream_input_counted(
            &config,
            "wire",
            None,
            &[InputItem::message(InputRole::User, "hello")],
            &[],
            100,
            &AtomicBool::new(false),
            &mut |_| {},
            &mut || std::future::ready(Ok(())),
        )
        .await
        .unwrap();
        peer.await.unwrap();
    }
}

#[tokio::test]
async fn go03_total_deadline_preserves_each_wire_output_and_one_attempt() {
    use protocol::Protocol;
    for protocol in [Protocol::Responses, Protocol::Chat, Protocol::Messages] {
        for (emitted, dispatch_delay_ms) in [(false, 0), (true, 0), (true, 250)] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let peer = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                loop {
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..end]);
                        let length: usize = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .map(str::to_owned)
                            })
                            .unwrap()
                            .parse()
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                if emitted {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.unwrap();
                    let start = match protocol {
                        Protocol::Responses => {
                            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"started\"}\n\n"
                        }
                        Protocol::Chat => {
                            "data: {\"choices\":[{\"delta\":{\"content\":\"started\"}}]}\n\n"
                        }
                        Protocol::Messages => {
                            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"role\":\"assistant\",\"usage\":{}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"started\"}}\n\n"
                        }
                    };
                    socket.write_all(start.as_bytes()).await.unwrap();
                    // Keep bytes flowing: a total deadline must not become an idle timeout.
                    for _ in 0..20 {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        if socket.write_all(b": ping\n\n").await.is_err() {
                            break;
                        }
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            });
            let config = ResponsesConfig {
                base_url: format!("http://{address}/prefix"),
                api_key: "synthetic".into(),
                timeout: None,
                // Disabling phase budgets must not disable this overall deadline.
                header_timeout_ms: None,
                chunk_timeout_ms: None,
                connect_timeout: Duration::from_secs(1),
                allow_private: true,
                headers: Default::default(),
                cache: crate::provider::CacheMode::None,
                wire: WireBinding {
                    protocol,
                    total_timeout_ms: Some(if dispatch_delay_ms == 0 { 120 } else { 400 }),
                    ..Default::default()
                },
            };
            let mut observed = false;
            let mut attempts = 0;
            let started = std::time::Instant::now();
            let result = stream_input_counted(
                &config,
                "wire",
                None,
                &[InputItem::message(InputRole::User, "hello")],
                &[],
                100,
                &AtomicBool::new(false),
                &mut |item| observed |= matches!(item, StreamItem::TextDelta(_)),
                &mut || {
                    attempts += 1;
                    async move {
                        tokio::time::sleep(Duration::from_millis(dispatch_delay_ms)).await;
                        Ok(())
                    }
                },
            )
            .await;
            if dispatch_delay_ms != 0 {
                assert!(
                    started.elapsed() < Duration::from_millis(525),
                    "overall deadline restarted after dispatch: {:?}",
                    started.elapsed()
                );
            }
            let error = result.unwrap_err();
            let ProviderError::Request(failure) = error else {
                panic!("{protocol:?} emitted={emitted}: expected typed deadline, got {error:?}");
            };
            assert_eq!(failure.transport, Some(TransportKind::Deadline));
            assert_eq!(failure.output_committed, emitted);
            assert_eq!(observed, emitted);
            assert_eq!(
                failure.operation,
                if emitted {
                    Operation::Read
                } else {
                    Operation::Request
                }
            );
            assert_eq!(
                failure.delivery,
                if emitted {
                    Delivery::Accepted
                } else {
                    Delivery::Unknown
                }
            );
            assert_eq!(attempts, 1);
            peer.await.unwrap();
        }
    }
}
