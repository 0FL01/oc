//! Local TUI smoke for T01.
//!
//! Dependency rule (D12): `oc-tui` depends on the public application API of
//! `oc-core` plus `oc-adapters` read-side domain (models/config/storage
//! reads and `tui.*` prefs). It never spawns network/process and leaves Db
//! handle lifecycle to the binary.

pub mod app;
pub mod commands;
pub mod dcp_panel;
pub mod events;
pub mod history;
pub mod picker;
pub mod smoke;
pub mod views;
pub mod workspace;

pub use smoke::{render_smoke_frame, tui_name};
