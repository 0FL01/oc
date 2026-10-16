//! Minimal compile smoke touching every T01 adapter dependency.
//!
//! Each function is intentionally tiny: it only proves the crate links and
//! the basic API shape works offline. Real storage/providers/tools/MCP/DCP
//! arrive in M1–M4.

use oc_core::application::AppHandle;

/// Adapter identity for diagnostics.
pub fn adapter_name() -> &'static str {
    "oc-adapters"
}

/// Open an in-memory SQLite database and prove a roundtrip.
///
/// Uses `rusqlite` with the `bundled` feature so no system SQLite is required.
pub fn smoke_memory_db() -> rusqlite::Result<i64> {
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute_batch("CREATE TABLE smoke(id INTEGER PRIMARY KEY, v TEXT NOT NULL);")?;
    conn.execute("INSERT INTO smoke(v) VALUES (?1)", ["hello"])?;
    let value: i64 = conn.query_row("SELECT COUNT(*) FROM smoke", [], |row| row.get(0))?;
    Ok(value)
}

/// Build an offline `reqwest` client without sending any request.
///
/// Proves the HTTP transport links. No network I/O happens here.
pub fn build_smoke_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("oc-t01-smoke")
        .build()
}

/// Reference the `rmcp` client types so the MCP SDK stays linked.
///
/// This does not start any child process or network connection; it only
/// proves the crate version resolves and its core types are reachable.
/// The full remote/stdio adapters arrive in M4 (T20/T21).
pub fn rmcp_smoke_marker() -> &'static str {
    // Pin the required MCP wire version from the goal contract.
    let version = rmcp::model::ProtocolVersion::V_2025_11_25;
    assert_eq!(version.as_str(), "2025-11-25");
    std::any::type_name::<rmcp::model::Tool>()
}

/// Touch the core application handle from the adapter layer.
pub fn describe_smoke_app(app: &AppHandle) -> String {
    format!("adapter:pending={}", app.pending_sessions())
}

#[cfg(test)]
mod tests {
    use super::{build_smoke_client, describe_smoke_app, rmcp_smoke_marker, smoke_memory_db};
    use oc_core::application::AppHandle;

    #[test]
    fn sqlite_memory_roundtrip() {
        assert_eq!(smoke_memory_db().expect("sqlite smoke"), 1);
    }

    #[test]
    fn reqwest_client_builds_offline() {
        let client = build_smoke_client().expect("client builds");
        let req = client
            .get("http://127.0.0.1:9/smoke")
            .build()
            .expect("request builds");
        assert_eq!(req.method(), reqwest::Method::GET);
    }

    #[test]
    fn rmcp_types_are_linked() {
        let marker = rmcp_smoke_marker();
        assert!(marker.contains("Tool"), "marker={marker}");
        assert_eq!(
            rmcp::model::ProtocolVersion::V_2025_11_25.as_str(),
            "2025-11-25"
        );
    }

    #[test]
    fn adapter_sees_core_app() {
        let app = AppHandle::smoke();
        assert_eq!(describe_smoke_app(&app), "adapter:pending=0");
    }

    #[tokio::test]
    async fn tokio_runtime_available() {
        tokio::task::yield_now().await;
        assert_eq!(oc_core::runtime::smoke_tick().await, 1);
    }
}
