//! Local TUI view-model for `oc`.
//!
//! Dependency rule (D12): `oc-tui` depends on the public application API of
//! `oc-core` plus the read-only domain helpers of `oc-adapters`
//! (`models`, `config`, `patch`). It never opens storage, never persists
//! preferences itself and never spawns network/process: persistence and
//! intent application stay in the `oc` binary.

pub mod app;
pub mod approval_view;
mod autocomplete;
mod child_view;
pub mod commands;
mod composer;
pub mod dcp_panel;
pub mod dcp_view;
pub mod dialog;
mod editor;
pub mod events;
mod fuzzy;
pub mod history;
pub mod layout;
pub mod messages;
mod patch_view;
pub mod picker;
pub mod question_view;
mod scanner;
pub mod shell;
mod shell_jobs_view;
pub mod smoke;
pub mod styled;
pub mod terminal;
pub mod terminal_view;
pub mod theme;
pub mod tools;
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
pub mod compaction;
