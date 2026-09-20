//! Local TUI smoke for T01.
//!
//! Dependency rule: `oc-tui` depends on the public application API of
//! `oc-core`, plus Ratatui/Crossterm. It must not depend on
//! `oc-adapters` storage/providers.

pub mod smoke;

pub use smoke::{render_smoke_frame, tui_name};
