//! Adapter smoke for T01.
//!
//! Proves dependency directions: `oc-adapters` implements `oc-core` ports
//! using Tokio + rusqlite (bundled) + reqwest + rmcp, with no reverse
//! dependency from `oc-core`.

pub mod attachments;
pub mod config;
pub mod dcp;
pub mod discovery;
pub mod files;
pub mod models;
pub mod patch;
pub mod provider;
pub mod shell;
pub mod smoke;
pub mod storage;
pub mod tools;
pub mod webfetch;

pub use smoke::{adapter_name, build_smoke_client, rmcp_smoke_marker, smoke_memory_db};
