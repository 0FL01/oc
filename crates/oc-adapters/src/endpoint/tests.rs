use super::*;
use crate::discovery::{DiscoveryClient, ReqwestDiscoveryClient};
use std::time::Duration;

#[test]
fn go01_endpoint_scope_provenance_and_ip_boundaries() {
    let binding =
        EndpointBinding::admit("http://localhost:8123/prefix/v1/", true, "local config").unwrap();
    assert!(binding.contains(&parse("http://localhost:8123/prefix/v1/models").unwrap()));
    for url in [
        "https://localhost:8123/prefix/v1/models",
        "http://localhost:8124/prefix/v1/models",
        "http://127.0.0.1:8123/prefix/v1/models",
        "http://localhost:8123/prefix/v11/models",
        "http://localhost:8123/prefix/v1/../models",
    ] {
        assert!(!binding.contains(&parse(url).unwrap()), "{url}");
    }
    assert_ne!(
        binding,
        EndpointBinding::admit("http://localhost:8123/prefix/v1", true, "other config").unwrap()
    );
    for ip in [
        "127.0.0.1",
        "::1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "fd00::1",
        "::ffff:192.168.1.1",
    ] {
        let ip: IpAddr = ip.parse().unwrap();
        assert!(binding.allows(ip), "{ip}");
        assert!(!allowed(ip, false, false), "{ip}");
    }
    for ip in [
        "0.0.0.0",
        "169.254.169.254",
        "169.254.1.1",
        "224.0.0.1",
        "255.255.255.255",
        "100.64.0.1",
        "192.0.2.1",
        "::",
        "fe80::1",
        "ff02::1",
        "2001:db8::1",
        "::ffff:169.254.169.254",
    ] {
        let ip: IpAddr = ip.parse().unwrap();
        assert!(!binding.allows(ip), "{ip}");
    }
    assert!(allowed("8.8.8.8".parse().unwrap(), false, false));
    assert!(EndpointBinding::admit("http://169.254.169.254/v1", true, "local").is_err());
    assert!(EndpointBinding::admit("http://127.0.0.1/v1", false, "untrusted").is_err());
    assert!(!format!("{binding:?}").contains("localhost"));
}

#[tokio::test]
async fn go01_discovery_captured_scope_refuses_escape_and_redirect_without_forwarding_credentials()
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let trap = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "http://localhost:{}/prefix/v1",
        listener.local_addr().unwrap().port()
    );
    let mut config = crate::provider::ResponsesConfig {
        base_url: base.clone(),
        api_key: "FIXTURE_SECRET".into(),
        timeout: None,
        chunk_timeout_ms: 1000,
        connect_timeout: Duration::from_secs(1),
        allow_private: false,
        headers: Default::default(),
        set_cache_key: false,
        wire: Default::default(),
    };
    config.wire.endpoint = Some(EndpointBinding::admit(&base, true, "local config").unwrap());
    let client = ReqwestDiscoveryClient::captured(&config).unwrap();
    let mut headers = crate::provider::request_headers(&config).unwrap();
    headers.insert(
        "accept",
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    assert!(
        client
            .get(
                &format!("{base}/../escape"),
                &headers,
                Duration::from_secs(1)
            )
            .await
            .is_err()
    );
    let trap_addr = trap.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let size = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..size]).to_lowercase();
        assert!(request.starts_with("get /prefix/v1/models http/1.1"));
        assert!(request.contains("authorization: bearer fixture_secret"));
        stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{trap_addr}/trap\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        drop(stream);
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let size = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..size]).to_lowercase();
        assert!(request.starts_with("get /prefix/v1/models http/1.1"));
        assert!(!request.contains("authorization:") && !request.contains("x-api-key:"));
        let body = r#"{"object":"list","data":[{"id":"anonymous-model"}]}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    assert_eq!(
        client
            .get(&format!("{base}/models"), &headers, Duration::from_secs(2))
            .await
            .unwrap()
            .0,
        302
    );
    config.api_key.clear();
    config.wire.auth_policy = crate::auth::AuthPolicy::None;
    let client = ReqwestDiscoveryClient::captured(&config).unwrap();
    let outcome = crate::discovery::refresh_provider(
        &client,
        &config,
        &Default::default(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .await;
    assert!(outcome.replaced, "{outcome:?}");
    assert!(outcome.models.contains_key("anonymous-model"));
    server.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(80), trap.accept())
            .await
            .is_err()
    );
    assert!(
        resolve(&format!("{base}/models"), None, false)
            .await
            .is_err()
    );
}
