use super::*;
use crate::config::{ProviderEntry, ProviderTimeout, provider_wire};
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

#[test]
fn go03_numeric_timeout_admission_and_units_are_exact() {
    for timeout in [json!(false), json!(1234)] {
        let entry: ProviderEntry =
            serde_json::from_value(json!({"options":{"timeout":timeout,"chunkTimeout":6000000}}))
                .unwrap();
        let wire = provider_wire("fixture", &entry).unwrap();
        assert_eq!(wire.total_timeout_ms, timeout.as_u64());
    }
    for timeout in [json!(true), json!(0)] {
        let entry: ProviderEntry =
            serde_json::from_value(json!({"options":{"timeout":timeout}})).unwrap();
        assert!(provider_wire("fixture", &entry).is_err());
    }
    for timeout in [json!(-1), json!(1.5), json!("1234")] {
        assert!(
            serde_json::from_value::<ProviderEntry>(json!({"options":{"timeout":timeout}}))
                .is_err()
        );
    }
    let entry: ProviderEntry =
        serde_json::from_value(json!({"options":{"chunkTimeout":0}})).unwrap();
    assert!(provider_wire("fixture", &entry).is_err());
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
            chunk_timeout_ms: 1000,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: Default::default(),
            set_cache_key: false,
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
        for emitted in [false, true] {
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
                chunk_timeout_ms: 1000,
                connect_timeout: Duration::from_secs(1),
                allow_private: true,
                headers: Default::default(),
                set_cache_key: false,
                wire: WireBinding {
                    protocol,
                    total_timeout_ms: Some(120),
                    ..Default::default()
                },
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
