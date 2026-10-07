//! Terminal setup/restore for the real `oc tui` loop.
//!
//! Alternate screen + raw mode are always restored: the [`TerminalGuard`]
//! covers normal and error returns, and [`install_panic_hook`] covers
//! panics by restoring first and then chaining the previous hook. Restore
//! is idempotent and safe to call when the terminal was never entered.

use std::{
    io::{self, Write},
    sync::OnceLock,
};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen,
    disable_raw_mode, enable_raw_mode,
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
    let _ = restore_output(&mut std::io::stderr());
}

fn restore_output(output: &mut impl Write) -> io::Result<()> {
    crossterm::execute!(
        output,
        EndSynchronizedUpdate,
        crossterm::style::Print("\x1b]112\x07"),
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    )
}

/// The real frame output boundary. Ratatui still owns its buffers/cursor
/// bookkeeping; its internal Show-before-MoveTo is deferred until publication.
/// Synchronized output prevents partial presentation when supported. It is
/// advisory: terminals ignoring it still hide before any paint and show only
/// after the final active-input position, even with fragmented writes.
pub struct FrameBackend<W: Write> {
    output: FrameOutput<W>,
    show_after_paint: bool,
}

struct FrameOutput<W: Write> {
    output: W,
    pending: Option<Vec<u8>>,
}

impl<W: Write> Write for FrameOutput<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(pending) = &mut self.pending {
            pending.extend_from_slice(bytes);
            Ok(bytes.len())
        } else {
            self.output.write(bytes)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_some() {
            Ok(())
        } else {
            self.output.flush()
        }
    }
}

impl<W: Write> FrameBackend<W> {
    pub fn new(output: W) -> Self {
        Self {
            output: FrameOutput {
                output,
                pending: None,
            },
            show_after_paint: false,
        }
    }

    /// Include first background, resize/clear, color and content in one paint.
    pub fn begin_frame(&mut self) -> io::Result<()> {
        if self.output.pending.is_some() {
            return Err(io::Error::other("terminal frame already active"));
        }
        self.show_after_paint = false;
        self.output.pending = Some(Vec::new());
        crossterm::queue!(self.output, BeginSynchronizedUpdate, Hide)
    }

    /// Publish once. A failed or partial publication is never replayed on Drop.
    pub fn finish_frame(&mut self) -> io::Result<()> {
        if self.output.pending.is_none() {
            return Err(io::Error::other("terminal frame is not active"));
        }
        if self.show_after_paint {
            crossterm::queue!(self.output, Show)?;
        }
        crossterm::queue!(self.output, EndSynchronizedUpdate)?;
        let output = &mut self.output;
        let pending = output.pending.take().expect("active frame");
        output.output.write_all(&pending)?;
        output.output.flush()
    }

    // CrosstermBackend only holds its writer; all bookkeeping lives in Terminal.
    // Borrow it for each operation instead of enabling its unstable writer API.
    fn backend(&mut self) -> ratatui::backend::CrosstermBackend<&mut FrameOutput<W>> {
        ratatui::backend::CrosstermBackend::new(&mut self.output)
    }
}

impl<W: Write> Write for FrameBackend<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.output.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl<W: Write> ratatui::backend::Backend for FrameBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        self.backend().draw(content)
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        if self.output.pending.is_some() {
            self.show_after_paint = false;
            Ok(())
        } else {
            self.backend().hide_cursor()
        }
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        if self.output.pending.is_some() {
            self.show_after_paint = true;
            Ok(())
        } else {
            self.backend().show_cursor()
        }
    }
    fn get_cursor_position(&mut self) -> io::Result<ratatui::layout::Position> {
        self.backend().get_cursor_position()
    }
    fn set_cursor_position<P: Into<ratatui::layout::Position>>(
        &mut self,
        position: P,
    ) -> io::Result<()> {
        self.backend().set_cursor_position(position)
    }
    fn clear(&mut self) -> io::Result<()> {
        self.backend().clear()
    }
    fn clear_region(&mut self, clear_type: ratatui::backend::ClearType) -> io::Result<()> {
        self.backend().clear_region(clear_type)
    }
    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.backend().append_lines(n)
    }
    fn size(&self) -> io::Result<ratatui::layout::Size> {
        ratatui::backend::CrosstermBackend::new(io::sink()).size()
    }
    fn window_size(&mut self) -> io::Result<ratatui::backend::WindowSize> {
        self.backend().window_size()
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

#[cfg(test)]
mod tests;

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
