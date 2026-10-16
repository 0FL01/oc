//! Central runtime admission also protects standalone public constructors.
use super::*;
use oc_core::queries::ServiceCode;

#[tokio::test]
async fn ui07_declared_missing_credential_cannot_be_bypassed_by_turn_config() {
    let (harness, mut generation) = make_harness(allow_all());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    generation.providers.insert("test".into(), serde_json::from_value(serde_json::json!({
        "options":{"baseURL":url,"apiKey":""},"models":{"m":{"limit":{"context":32768,"output":4096}}}
    })).unwrap());
    let runtime = runtime_of(&harness, generation, vec![]);
    runtime.create_session("s").unwrap();
    let mut accepted = 0;
    let result = runtime
        .run_turn_with_events(
            params(
                "s",
                "must not be accepted",
                &harness,
                provider_of(&url),
                &NO_CANCEL,
            ),
            |_| accepted += 1,
            |_, _| {},
            |_, _| {},
        )
        .await;
    assert!(
        matches!(result, Err(RuntimeError::ProviderUnavailable(ref diagnostic)) if diagnostic.code == ServiceCode::MissingCredential)
    );
    assert_eq!(accepted, 0);
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let turns: i64 = sql
        .query_row("SELECT COUNT(*) FROM turns", [], |row| row.get(0))
        .unwrap();
    assert_eq!(turns, 0);
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    runtime.shutdown_mcp().await.unwrap();
}
