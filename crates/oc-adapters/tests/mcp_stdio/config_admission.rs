//! MCP09 normalized stage deadlines, redaction, and pinned SDK capability spike.
use super::*;
use oc_adapters::config::{Source, assemble};
use serde_json::{Value, json};

struct Peer {
    _root: tempfile::TempDir,
    report: PathBuf,
    config: StdioConfig,
}

impl Peer {
    fn new(stage: &str, timeout: Value) -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let server = bin.join("mcp09-server");
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mcp09-stdio.py"),
            &server,
        )
        .unwrap();
        fs::set_permissions(&server, fs::Permissions::from_mode(0o700)).unwrap();
        let report = root.path().join("effects.json");
        let source = Source { path: root.path().join("opencode.json").to_string_lossy().into_owned(), trusted:true, text:json!({"mcp":{"probe":{
            "type":"local","command":["mcp09-server",report,root.path(),"project",stage,"two words",""],
            "environment":{"PATH":bin,"MCP09_OVERLAY":"mcp09-configured-canary"},"timeout":timeout
        }}}).to_string() };
        let env = BTreeMap::from([
            ("MCP09_INHERITED".into(), "mcp09-inherited-canary".into()),
            ("GLOBAL_PROVIDER_TOKEN".into(), "mcp09-domain-canary".into()),
            ("BENIGN_ALIAS".into(), "mcp09-domain-canary".into()),
            ("PATH".into(), "/bad-parent-path".into()),
        ]);
        let generation = assemble(&[source], &env, None).unwrap();
        let config =
            StdioConfig::from_entry("probe", &generation.mcp["probe"], root.path(), &env).unwrap();
        Self {
            _root: root,
            report,
            config,
        }
    }

    fn counters(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.report).unwrap()).unwrap()
    }

    fn assert_reaped(&self) {
        let pid = self.counters()["pid"].as_i64().unwrap() as libc::pid_t;
        // SAFETY: probe only this fixture-owned child, not any runner process.
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
        let mut status = 0;
        // SAFETY: only this fixture's child; status is valid writable memory.
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        assert_eq!(waited, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}

#[tokio::test]
async fn mcp09_separate_actual_startup_catalog_execution_deadlines_and_reaping() {
    for stage in ["initialize", "catalog", "execution"] {
        let mut timeout = json!({"startup":2000,"catalog":2000,"execution":2000});
        timeout[if stage == "initialize" {
            "startup"
        } else {
            stage
        }] = json!(100);
        let peer = Peer::new(stage, timeout);
        let cancel = AtomicBool::new(false);
        let began = Instant::now();
        let launch = StdioClient::launch(&peer.config).await;
        if stage == "initialize" {
            assert!(matches!(launch, Err(StdioError::Deadline)));
        } else {
            let client = launch.unwrap();
            let list = client.list_tools(&cancel).await;
            if stage == "catalog" {
                assert_eq!(list, Err(StdioError::Deadline));
            } else {
                assert_eq!(list.unwrap().len(), 1);
                assert_eq!(
                    client.call_tool("probe", json!({}), &cancel).await,
                    Err(StdioError::Deadline)
                );
            }
            client.shutdown().await.unwrap();
        }
        assert!(
            began.elapsed() < Duration::from_secs(1),
            "independent cleanup budget exceeded"
        );
        let counters = peer.counters();
        assert_eq!(counters["initialize"], 1);
        assert_eq!(
            counters["catalog"],
            if stage == "initialize" { 0 } else { 1 }
        );
        assert_eq!(counters["call"], if stage == "execution" { 1 } else { 0 });
        peer.assert_reaped();
    }
}

#[tokio::test]
async fn mcp09_numeric_timeout_does_not_bound_startup_and_canaries_stay_redacted() {
    let peer = Peer::new("legacy-delay", json!(40));
    assert_eq!(peer.config.startup_timeout, Some(Duration::from_secs(30)));
    assert_eq!(peer.config.catalog_timeout, Some(Duration::from_millis(40)));
    assert_eq!(peer.config.timeout, Duration::from_millis(40));
    let began = Instant::now();
    let client = StdioClient::launch(&peer.config).await.unwrap();
    assert!(began.elapsed() >= Duration::from_millis(200));
    let cancel = AtomicBool::new(false);
    client.list_tools(&cancel).await.unwrap();
    let result = client.call_tool("probe", json!({}), &cancel).await.unwrap();
    assert!(result.contains("probe complete"));
    // Wait for the owned stderr reader, without a raw environment/log dump.
    let stderr = tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            let stderr = client.stderr_snapshot();
            if stderr.text.contains("probe complete") {
                break stderr;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    for canary in [
        "mcp09-configured-canary",
        "mcp09-inherited-canary",
        "mcp09-domain-canary",
    ] {
        assert!(!result.contains(canary));
        assert!(!client.instructions().unwrap().contains(canary));
        assert!(!stderr.text.contains(canary));
        assert!(!format!("{:?}", peer.config).contains(canary));
    }
    for field in [
        "argv",
        "cwd",
        "inherited",
        "overlay",
        "path",
        "credentials",
        "alias",
    ] {
        assert_eq!(peer.counters()[field], true, "{field}");
    }
    client.shutdown().await.unwrap();
    peer.assert_reaped();
}

#[tokio::test]
async fn mcp09_modern_initialize_response_is_not_a_qualified_native_protocol_path() {
    let peer = Peer::new("modern", json!({"startup":1000}));
    assert!(matches!(
        StdioClient::launch(&peer.config).await,
        Err(StdioError::ProtocolMismatch)
    ));
    assert_eq!(peer.counters()["initialize"], 1);
    assert_eq!(peer.counters()["catalog"], 0);
    peer.assert_reaped();
}

/// rmcp 3.4.0 knows modern discovery. Our current adapters deliberately use
/// Initialize, so SDK support alone cannot turn `protocol:auto/modern` green.
#[tokio::test]
async fn mcp09_pinned_rmcp_compatibility_spike_legacy_discover_and_auto() {
    use rmcp::ClientLifecycleMode;
    use rmcp::model::ProtocolVersion;
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
    for (mode, method, version) in [
        (ClientLifecycleMode::Initialize, "initialize", "2025-11-25"),
        (
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
            "server/discover",
            "2026-07-28",
        ),
        (
            ClientLifecycleMode::Auto {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                legacy_version: Some(ProtocolVersion::V_2025_11_25),
            },
            "server/discover",
            "2026-07-28",
        ),
    ] {
        let (server, client) = tokio::io::duplex(8192);
        let peer = async move {
            let mut server = tokio::io::BufReader::new(server);
            let mut line = String::new();
            server.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], method);
            let result = if method == "initialize" {
                assert_eq!(request["params"]["protocolVersion"], "2025-11-25");
                json!({"protocolVersion":version,"capabilities":{},"serverInfo":{"name":"spike","version":"1"}})
            } else {
                assert_eq!(
                    request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
                    "2026-07-28"
                );
                serde_json::to_value(
                    rmcp::model::DiscoverResult::new(
                        vec![ProtocolVersion::V_2026_07_28],
                        Default::default(),
                    )
                    .with_server_info(rmcp::model::Implementation::new("spike", "1")),
                )
                .unwrap()
            };
            server
                .write_all(
                    format!(
                        "{}\n",
                        json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            if method == "initialize" {
                line.clear();
                server.read_line(&mut line).await.unwrap();
                assert_eq!(
                    serde_json::from_str::<Value>(&line).unwrap()["method"],
                    "notifications/initialized"
                );
            }
        };
        let client = async move {
            let client = rmcp::service::serve_client_with_lifecycle((), client, mode)
                .await
                .unwrap();
            assert_eq!(
                client.peer_info().unwrap().protocol_version.as_str(),
                version
            );
            client.cancel().await.unwrap();
        };
        tokio::time::timeout(TEST_TIMEOUT, async {
            tokio::join!(peer, client);
        })
        .await
        .unwrap();
    }
}
