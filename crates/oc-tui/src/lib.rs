//! Local TUI smoke for T01.
//!
//! Dependency rule: `oc-tui` depends on the public application API of
//! `oc-core`, plus Ratatui/Crossterm. It must not depend on
//! `oc-adapters` storage/providers.

pub mod app;
pub mod events;
pub mod smoke;
pub mod views;

pub use smoke::{render_smoke_frame, tui_name};
