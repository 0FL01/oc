//! Native CoreApp caller proof over actual loopback transport/resource ownership.
use super::*;
use oc_core::{
    domain::SessionId,
    queries::{McpLookup, McpLookupError, McpLookupOp, McpStatus},
};
use std::collections::BTreeMap;

async fn app(f: &Fixture) -> (oc_core::core_app::CoreApp, oc_core::core_app::WorkerGuard) {
    let env = BTreeMap::from([
        ("HOME".into(), f.home.display().to_string()),
        (
            "XDG_CONFIG_HOME".into(),
            f.home.join("config").display().to_string(),
        ),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&f.project, &f.home.join("lookup-data"), env)
            .await
            .unwrap();
    app.create_session(SessionId("native-lookup-root".into()))
        .await
        .unwrap();
    (app, guard)
}
async fn read_query(app: &oc_core::core_app::CoreApp) -> McpLookup {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let snapshot = app.mcp_status().await.unwrap();
        if let Some(row) = snapshot
            .servers
            .iter()
            .find(|row| row.status == McpStatus::Connected)
        {
            return McpLookup {
                binding: snapshot.binding,
                server: row.id.clone(),
                session: SessionId("native-lookup-root".into()),
                operation: McpLookupOp::ReadResource {
                    uri: "fixture://held".into(),
                },
                refresh: true,
            };
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(POLL).await;
    }
}
async fn wait_rpc(mcp: &FakeMcp, method: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while !mcp
        .records()
        .iter()
        .any(|record| record.rpc_method == method)
    {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(POLL).await;
    }
}

#[tokio::test]
async fn mcp11_native_core_remote_drop_cancels_exact_id_quarantines_body_and_closes_owned_http() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let mcp = FakeMcp::stalled_request("resources/read", false);
    let f = Fixture::new();
    f.write_config(
        &responses,
        json!({"peer":{"type":"remote","url":mcp.url,"oauth":false}}),
        json!({"mcp_lookup":"allow","read":"allow"}),
    );
    let (app, guard) = app(&f).await;
    let q = read_query(&app).await;
    let pending = tokio::spawn({
        let app = app.clone();
        let q = q.clone();
        async move { app.mcp_lookup(q).await }
    });
    wait_rpc(&mcp, "resources/read").await;
    tokio::time::timeout(Duration::from_millis(500), app.mcp_status())
        .await
        .unwrap()
        .unwrap();
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    wait_rpc(&mcp, "notifications/cancelled").await;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !app.mcp_status().await.unwrap().servers.iter().any(|row| {
        row.diagnostic
            .as_ref()
            .is_some_and(|d| d.code == oc_core::queries::ServiceCode::UnsafeRetry)
    }) {
        assert!(Instant::now() < deadline, "body was not quarantined");
        tokio::time::sleep(POLL).await;
    }
    assert_eq!(app.mcp_lookup(q).await, Err(McpLookupError::UnsafeRetry));
    let records = mcp.records();
    let read = records
        .iter()
        .find(|r| r.rpc_method == "resources/read")
        .unwrap();
    assert!(
        records
            .iter()
            .any(|r| r.rpc_method == "notifications/cancelled"
                && r.arguments.as_ref().unwrap()["requestId"] == read.request_id.clone().unwrap())
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r.rpc_method == "resources/read")
            .count(),
        1,
        "no unknown-outcome replay"
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r.rpc_method == "initialize")
            .count(),
        1
    );
    assert!(
        responses.requests().is_empty(),
        "lookup never invokes model/title generation"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !mcp.call_socket_closed.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(POLL).await;
    }
}

#[tokio::test]
async fn mcp11_native_core_failed_lookup_cancellation_is_fatal_not_usable_empty_success() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let mcp = FakeMcp::stalled_request("resources/read", true);
    let f = Fixture::new();
    f.write_config(
        &responses,
        json!({"peer":{"type":"remote","url":mcp.url,"oauth":false}}),
        json!({"mcp_lookup":"allow","read":"allow"}),
    );
    let (app, guard) = app(&f).await;
    let q = read_query(&app).await;
    let pending = tokio::spawn({
        let app = app.clone();
        async move { app.mcp_lookup(q).await }
    });
    wait_rpc(&mcp, "resources/read").await;
    pending.abort();
    let _ = pending.await;
    wait_rpc(&mcp, "notifications/cancelled").await;
    assert!(
        tokio::time::timeout(Duration::from_secs(12), guard.join())
            .await
            .unwrap()
            .is_err(),
        "unconfirmed cleanup is non-success"
    );
    assert!(responses.requests().is_empty());
    assert_eq!(
        mcp.records()
            .iter()
            .filter(|r| r.rpc_method == "resources/read")
            .count(),
        1
    );
}
