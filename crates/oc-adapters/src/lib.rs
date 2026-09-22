//! Adapter smoke for T01.
//!
//! Proves dependency directions: `oc-adapters` implements `oc-core` ports
//! using Tokio + rusqlite (bundled) + reqwest + rmcp, with no reverse
//! dependency from `oc-core`.

pub mod application;
pub mod attachments;
pub mod composition;
pub mod config;
pub mod dcp;
pub mod dcp_auto;
pub mod defs;
pub mod discovery;
pub mod files;
pub mod mcp_remote;
pub mod mcp_stdio;
pub mod models;
pub mod patch;
pub mod provider;
pub mod runtime;
pub mod shell;
pub mod smoke;
pub mod storage;
pub mod tools;
pub mod tui_workspace;
pub mod webfetch;

/// Outbound `User-Agent` for the native HTTP clients (`oc/<version>`).
///
/// JS runtimes always send a default `User-Agent`; `reqwest` sends none, and
/// frontends such as Cloudflare answer `403 Error 1010` to UA-less requests
/// (observed before the configured remote MCP endpoint).
pub const USER_AGENT: &str = concat!("oc/", env!("CARGO_PKG_VERSION"));

/// `User-Agent` for page fetches: browser-like on purpose, matching upstream
/// opencode's `OpenCode-User/1.0` intent because sites block UA-less clients.
pub const WEB_USER_AGENT: &str = "oc-user/1.0";

pub use smoke::{adapter_name, build_smoke_client, rmcp_smoke_marker, smoke_memory_db};
