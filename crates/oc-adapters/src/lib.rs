//! Adapter smoke for T01.
//!
//! Proves dependency directions: `oc-adapters` implements `oc-core` ports
//! using Tokio + rusqlite (bundled) + reqwest + rmcp, with no reverse
//! dependency from `oc-core`.

pub mod config;
pub mod files;
pub mod smoke;
pub mod storage;

pub use smoke::{adapter_name, build_smoke_client, rmcp_smoke_marker, smoke_memory_db};
