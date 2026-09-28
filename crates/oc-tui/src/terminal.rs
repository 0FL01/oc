//! Terminal setup/restore for the real `oc tui` loop.
//!
//! Alternate screen + raw mode are always restored: the [`TerminalGuard`]
//! covers normal and error returns, and [`install_panic_hook`] covers
//! panics by restoring first and then chaining the previous hook. Restore
//! is idempotent and safe to call when the terminal was never entered.

use std::{io::Write, sync::OnceLock};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Guard that restores the terminal on drop (normal and error paths).
pub struct TerminalGuard {
    _private: (),
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Enter alternate screen + raw mode; the guard restores on drop.
pub fn enter() -> Result<TerminalGuard, String> {
    enable_raw_mode().map_err(|e| format!("raw mode: {e}"))?;
    let mut stderr = std::io::stderr();
    if let Err(e) = crossterm::execute!(stderr, EnterAlternateScreen, EnableMouseCapture) {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), DisableMouseCapture, LeaveAlternateScreen);
        return Err(format!("screen: {e}"));
    }
    Ok(TerminalGuard { _private: () })
}

/// Restore cooked mode and leave the alternate screen. Idempotent: safe to
/// call when the terminal was never entered (errors ignored).
pub fn restore() {
    let _ = disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stderr(),
        crossterm::style::Print("\x1b]112\x07"),
        DisableMouseCapture,
        LeaveAlternateScreen
    );
}

/// Cursor paint is terminal state, outside Ratatui's styled cell buffer. Reset
/// returns to the terminal's default; restore also covers errors and panics.
pub fn set_cursor_color(
    output: &mut impl Write,
    color: ratatui::style::Color,
) -> std::io::Result<()> {
    match color {
        ratatui::style::Color::Rgb(r, g, b) => {
            write!(output, "\x1b]12;#{r:02x}{g:02x}{b:02x}\x07")
        }
        _ => output.write_all(b"\x1b]112\x07"),
    }
}

static PANIC_HOOK_ONCE: OnceLock<()> = OnceLock::new();

/// Install a panic hook that restores the terminal before chaining the
/// previous hook, so a panic inside `oc tui` never leaves the user's
/// terminal raw or on the alternate screen.
pub fn install_panic_hook() {
    PANIC_HOOK_ONCE.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
    });
}
