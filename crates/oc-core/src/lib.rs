//! Typed application core for `oc`.
//!
//! Dependency rule (see `docs/ARCHITECTURE.md`): `oc-core` may use
//! Tokio/Serde and small utility types, but must not depend on
//! Ratatui, rusqlite, reqwest or rmcp. UI/storage/network live in
//! `oc-adapters` / `oc-tui` / `oc`.

pub mod application;
pub mod domain;
pub mod ports;
pub mod runtime;

/// Crate identity used by smoke tests and diagnostics.
pub const CORE_NAME: &str = "oc-core";

/// Minimal core version string; workspace version is authoritative for releases.
pub fn core_name() -> &'static str {
    CORE_NAME
}

#[cfg(test)]
mod tests {
    use super::core_name;

    #[test]
    fn core_name_is_stable() {
        assert_eq!(core_name(), "oc-core");
    }
}
