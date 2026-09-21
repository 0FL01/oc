//! Local TUI view-model for `oc`.
//!
//! Dependency rule (D12): `oc-tui` depends on the public application API of
//! `oc-core` plus the read-only domain helpers of `oc-adapters`
//! (`models`, `config`, `patch`). It never opens storage, never persists
//! preferences itself and never spawns network/process: persistence and
//! intent application stay in the `oc` binary.

pub mod app;
pub mod commands;
pub mod dcp_panel;
pub mod events;
pub mod history;
pub mod picker;
pub mod smoke;
pub mod styled;
pub mod terminal;
pub mod theme;
pub mod views;

pub use smoke::{render_smoke_frame, tui_name};

/// Truncate `text` to at most `max` bytes on a UTF-8 char boundary.
pub(crate) fn truncate_utf8(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
