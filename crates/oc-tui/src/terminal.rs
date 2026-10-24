//! Terminal setup/restore for the real `oc tui` loop.
//!
//! Alternate screen + raw mode are always restored: the [`TerminalGuard`]
//! covers normal and error returns, and [`install_panic_hook`] covers
//! panics by restoring first and then chaining the previous hook. Restore
//! is idempotent and safe to call when the terminal was never entered.

use std::{
    io::{self, Write},
    sync::{Mutex, OnceLock},
};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture,
};
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
    if let Err(e) = crossterm::execute!(
        stderr,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableFocusChange,
        EnableBracketedPaste
    ) {
        let _ = disable_raw_mode();
        let _ = restore_output(&mut std::io::stderr());
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
        DisableFocusChange,
        DisableBracketedPaste,
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
    sizes: Mutex<FrameSizes>,
    /// What the terminal shows, while known (from a cleared screen on).
    screen: Option<scroll::Screen>,
    /// The terminal is blank: the next draw may start tracking it.
    cleared: bool,
}

#[derive(Default)]
struct FrameSizes {
    observed: Option<ratatui::layout::Size>,
    resized: Option<ratatui::layout::Size>,
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
            sizes: Mutex::new(FrameSizes::default()),
            screen: None,
            cleared: true,
        }
    }

    /// Treat `size` as the observed full-screen size, for an output that is
    /// not the controlling terminal (benchmarks, fixtures).
    pub fn assume_size(&self, size: ratatui::layout::Size) -> io::Result<()> {
        self.observe_size(size).map(|_| ())
    }

    /// The tracked screen for this draw at the observed full-screen size.
    fn take_screen(&mut self) -> io::Result<Option<scroll::Screen>> {
        let observed = self
            .sizes
            .get_mut()
            .map_err(|_| io::Error::other("terminal size state poisoned"))?
            .observed;
        let screen = match (self.screen.take(), observed) {
            (Some(screen), Some(size)) if screen.size() == size => Some(screen),
            (None, Some(size)) if self.cleared => Some(scroll::Screen::blank(size)),
            _ => None,
        };
        self.cleared = false;
        Ok(screen)
    }

    fn forget_screen(&mut self, cleared: bool) {
        self.screen = None;
        self.cleared = cleared;
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
        self.sizes
            .get_mut()
            .map_err(|_| io::Error::other("terminal size state poisoned"))?
            .resized = None;
        let output = &mut self.output;
        let pending = output.pending.take().expect("active frame");
        let published = output
            .output
            .write_all(&pending)
            .and_then(|()| output.output.flush());
        if published.is_err() {
            self.forget_screen(false);
        }
        published
    }

    // CrosstermBackend only holds its writer; all bookkeeping lives in Terminal.
    // Borrow it for each operation instead of enabling its unstable writer API.
    fn backend(&mut self) -> ratatui::backend::CrosstermBackend<&mut FrameOutput<W>> {
        ratatui::backend::CrosstermBackend::new(&mut self.output)
    }

    fn observe_size(&self, size: ratatui::layout::Size) -> io::Result<ratatui::layout::Size> {
        let mut sizes = self
            .sizes
            .lock()
            .map_err(|_| io::Error::other("terminal size state poisoned"))?;
        if sizes.observed.is_some_and(|previous| previous != size) {
            sizes.resized = Some(size);
        }
        sizes.observed = Some(size);
        Ok(size)
    }

    fn clear_resized_viewport(&mut self) -> io::Result<bool> {
        let resized = self
            .sizes
            .get_mut()
            .map_err(|_| io::Error::other("terminal size state poisoned"))?
            .resized;
        let Some(size) = resized.filter(|_| self.output.pending.is_some()) else {
            return Ok(false);
        };
        if size.width == 0 || size.height == 0 {
            return Ok(true); // No visible cells, and no whole-screen fallback.
        }
        // Alternate-screen shrink may retain cells beyond the new column count.
        // ED clears those too. Erase every visible cell instead, while Ratatui
        // still invalidates its buffers and repaints the entire new viewport.
        // Like Clear(All), keep the caller's cursor and current erase attributes;
        // all operations remain inside the hidden synchronized frame.
        crossterm::queue!(self.output, crossterm::cursor::SavePosition)?;
        let erase = format!("\x1b[{}X", size.width);
        for row in 0..size.height {
            crossterm::queue!(
                self.output,
                crossterm::cursor::MoveTo(0, row),
                crossterm::style::Print(&erase)
            )?;
        }
        crossterm::queue!(self.output, crossterm::cursor::RestorePosition)?;
        Ok(true)
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
        let Some(mut screen) = self.take_screen()? else {
            return self.backend().draw(content);
        };
        let changes: Vec<_> = content.collect();
        if !screen.covers(&changes) {
            return self.backend().draw(changes.into_iter());
        }
        let rows = scroll::touched_rows(&changes);
        if rows.len() < scroll::MIN_ROWS {
            screen.apply(&changes, &rows);
            self.backend().draw(changes.into_iter())?;
        } else {
            let previous = screen.clone();
            screen.apply(&changes, &rows);
            if let Some(plan) = previous.plan(&screen) {
                self.output.write_all(plan.escape().as_bytes())?;
                let scrolled = previous.scrolled(plan);
                self.backend()
                    .draw(scrolled.diff(screen.buffer()).into_iter())?;
            } else {
                self.backend().draw(changes.into_iter())?;
            }
        }
        self.screen = Some(screen);
        Ok(())
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
        self.forget_screen(false);
        if !self.clear_resized_viewport()? {
            self.backend().clear()?;
        }
        self.forget_screen(true);
        Ok(())
    }
    fn clear_region(&mut self, clear_type: ratatui::backend::ClearType) -> io::Result<()> {
        let all = matches!(&clear_type, ratatui::backend::ClearType::All);
        self.forget_screen(false);
        if !(all && self.clear_resized_viewport()?) {
            self.backend().clear_region(clear_type)?;
        }
        self.forget_screen(all);
        Ok(())
    }
    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.forget_screen(false);
        self.backend().append_lines(n)
    }
    fn size(&self) -> io::Result<ratatui::layout::Size> {
        self.observe_size(ratatui::backend::CrosstermBackend::new(io::sink()).size()?)
    }
    fn window_size(&mut self) -> io::Result<ratatui::backend::WindowSize> {
        self.backend().window_size()
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

mod scroll;
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
