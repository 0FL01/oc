use super::*;
use ratatui::{
    Terminal, TerminalOptions, Viewport, backend::Backend, layout::Rect, widgets::Paragraph,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Recorded {
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
}

struct Output {
    recorded: Rc<RefCell<Recorded>>,
    fragment: usize,
    fail_at: Option<usize>,
    fail_flush: bool,
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut recorded = self.recorded.borrow_mut();
        if self.fail_at.is_some_and(|at| recorded.bytes.len() >= at) {
            return Err(io::Error::other("fixture write failure"));
        }
        let count = bytes.len().min(self.fragment).min(
            self.fail_at
                .map_or(usize::MAX, |at| at.saturating_sub(recorded.bytes.len())),
        );
        recorded.bytes.extend_from_slice(&bytes[..count]);
        recorded.writes += 1;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.recorded.borrow_mut().flushes += 1;
        if self.fail_flush {
            Err(io::Error::other("fixture flush failure"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn fallback_real_backend_defers_show_through_background_clear_diff_and_final_position() {
    for fragment in [1, 7, usize::MAX] {
        let recorded = Rc::new(RefCell::new(Recorded::default()));
        let backend = FrameBackend::new(Output {
            recorded: recorded.clone(),
            fragment,
            fail_at: None,
            fail_flush: false,
        });
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, 20, 4)),
            },
        )
        .unwrap();
        terminal.backend_mut().begin_frame().unwrap();
        terminal
            .backend_mut()
            .clear_region(ratatui::backend::ClearType::All)
            .unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new("background"), frame.area());
            })
            .unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new("tool Ω界\nprompt draft"), frame.area());
                frame.set_cursor_position((4, 2));
            })
            .unwrap();
        assert!(
            recorded.borrow().bytes.is_empty(),
            "internal flush must not publish"
        );
        terminal.backend_mut().finish_frame().unwrap();
        let bytes = recorded.borrow().bytes.clone();
        assert!(bytes.starts_with(b"\x1b[?2026h\x1b[?25l\x1b[2J"));
        assert!(bytes.ends_with(b"\x1b[3;5H\x1b[?25h\x1b[?2026l"));
        let show_count = bytes
            .windows(6)
            .filter(|part| *part == b"\x1b[?25h")
            .count();
        assert_eq!(
            show_count, 1,
            "no complete Show at any repaint command boundary"
        );
        // The same ordering is safe if a terminal ignores the advisory 2026
        // envelope: no paint precedes Hide and no Show precedes final MoveTo.
        assert_eq!(recorded.borrow().flushes, 1);

        // Read-only paint must not retain the preceding frame's input owner.
        let start = recorded.borrow().bytes.len();
        terminal.backend_mut().begin_frame().unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new("read-only result"), frame.area());
            })
            .unwrap();
        terminal.backend_mut().finish_frame().unwrap();
        assert!(
            !recorded.borrow().bytes[start..]
                .windows(6)
                .any(|part| part == b"\x1b[?25h")
        );
    }
}

#[test]
fn partial_write_or_flush_error_is_non_success_and_never_replays_the_frame() {
    for (fail_at, fail_flush) in [(Some(3), false), (None, true)] {
        let recorded = Rc::new(RefCell::new(Recorded::default()));
        let mut backend = FrameBackend::new(Output {
            recorded: recorded.clone(),
            fragment: 1,
            fail_at,
            fail_flush,
        });
        backend.begin_frame().unwrap();
        backend.clear().unwrap();
        backend.show_cursor().unwrap();
        backend.set_cursor_position((4, 2)).unwrap();
        assert!(backend.finish_frame().is_err());
        let written = recorded.borrow().bytes.clone();
        assert!(
            backend.finish_frame().is_err(),
            "failed publication is not pending"
        );
        drop(backend);
        assert_eq!(recorded.borrow().bytes, written);
    }
}

#[test]
fn restore_output_ends_partial_sync_before_releasing_modes_and_showing_cursor() {
    let mut bytes = Vec::new();
    restore_output(&mut bytes).unwrap();
    assert!(bytes.starts_with(b"\x1b[?2026l\x1b]112\x07"));
    assert!(bytes.ends_with(b"\x1b[?1049l\x1b[?25h"));
    assert!(bytes.windows(8).any(|part| part == b"\x1b[?1006l"));
}
