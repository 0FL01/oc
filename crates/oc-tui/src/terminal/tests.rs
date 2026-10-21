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
    for (fail_at, fail_flush, resized) in [
        (Some(3), false, false),
        (None, true, false),
        (Some(3), false, true),
        (None, true, true),
    ] {
        let recorded = Rc::new(RefCell::new(Recorded::default()));
        let mut backend = FrameBackend::new(Output {
            recorded: recorded.clone(),
            fragment: 1,
            fail_at,
            fail_flush,
        });
        if resized {
            backend
                .observe_size(ratatui::layout::Size::new(40, 8))
                .unwrap();
        }
        backend.begin_frame().unwrap();
        if resized {
            backend
                .observe_size(ratatui::layout::Size::new(20, 4))
                .unwrap();
        }
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
fn resized_frame_clears_every_visible_cell_without_erasing_retained_columns() {
    let recorded = Rc::new(RefCell::new(Recorded::default()));
    let mut backend = FrameBackend::new(Output {
        recorded: recorded.clone(),
        fragment: 1,
        fail_at: None,
        fail_flush: false,
    });
    backend
        .observe_size(ratatui::layout::Size::new(120, 320))
        .unwrap();
    backend.begin_frame().unwrap();
    backend
        .observe_size(ratatui::layout::Size::new(80, 24))
        .unwrap();
    backend.clear().unwrap();
    // Ratatui can clear again while resizing its viewport; both are scoped.
    backend
        .clear_region(ratatui::backend::ClearType::All)
        .unwrap();
    backend.set_cursor_position((4, 2)).unwrap();
    backend.show_cursor().unwrap();
    assert!(recorded.borrow().bytes.is_empty());
    backend.finish_frame().unwrap();
    let bytes = recorded.borrow().bytes.clone();
    assert!(!bytes.windows(4).any(|part| part == b"\x1b[2J"));
    for row in 1..=24 {
        let command = format!("\x1b[{row};1H\x1b[80X");
        assert_eq!(
            bytes
                .windows(command.len())
                .filter(|part| *part == command.as_bytes())
                .count(),
            2,
            "each clear must erase all 80 visible columns of row {row}"
        );
    }
    assert!(bytes.starts_with(b"\x1b[?2026h\x1b[?25l"));
    assert!(bytes.ends_with(b"\x1b[3;5H\x1b[?25h\x1b[?2026l"));
    assert_eq!(recorded.borrow().flushes, 1);

    // Clear(All) must preserve the cursor even with no subsequent draw/MoveTo.
    let start = bytes.len();
    backend.begin_frame().unwrap();
    crossterm::queue!(
        backend,
        crossterm::style::SetBackgroundColor(crossterm::style::Color::Rgb { r: 1, g: 2, b: 3 })
    )
    .unwrap();
    backend.set_cursor_position((7, 3)).unwrap();
    backend
        .observe_size(ratatui::layout::Size::new(79, 24))
        .unwrap();
    backend.clear().unwrap();
    backend.show_cursor().unwrap();
    backend.finish_frame().unwrap();
    assert!(
        recorded.borrow().bytes[start..]
            .windows(8)
            .any(|part| part == b"\x1b[4;8H\x1b7")
    );
    assert!(
        recorded
            .borrow()
            .bytes
            .ends_with(b"\x1b8\x1b[?25h\x1b[?2026l")
    );
    assert!(
        !recorded.borrow().bytes[start..]
            .windows(4)
            .any(|part| part == b"\x1b[0m"),
        "visible erasure must inherit the caller's attributes, as ED did"
    );

    // Stable-size/manual clear still uses the original whole-screen operation.
    let start = recorded.borrow().bytes.len();
    backend.begin_frame().unwrap();
    backend
        .observe_size(ratatui::layout::Size::new(79, 24))
        .unwrap();
    backend.clear().unwrap();
    backend.finish_frame().unwrap();
    assert!(
        recorded.borrow().bytes[start..]
            .windows(4)
            .any(|part| part == b"\x1b[2J")
    );
    for size in [
        ratatui::layout::Size::new(0, 24),
        ratatui::layout::Size::new(80, 0),
    ] {
        let start = recorded.borrow().bytes.len();
        backend.begin_frame().unwrap();
        backend.observe_size(size).unwrap();
        backend.clear().unwrap();
        backend
            .clear_region(ratatui::backend::ClearType::All)
            .unwrap();
        backend.finish_frame().unwrap();
        assert_eq!(
            &recorded.borrow().bytes[start..],
            b"\x1b[?2026h\x1b[?25l\x1b[?2026l",
            "zero-sized viewport cannot erase retained cells"
        );
    }
}

#[test]
fn restore_output_ends_partial_sync_before_releasing_modes_and_showing_cursor() {
    let mut bytes = Vec::new();
    restore_output(&mut bytes).unwrap();
    assert!(bytes.starts_with(b"\x1b[?2026l\x1b]112\x07"));
    assert!(bytes.ends_with(b"\x1b[?1049l\x1b[?25h"));
    assert!(bytes.windows(8).any(|part| part == b"\x1b[?1006l"));
    assert!(bytes.windows(8).any(|part| part == b"\x1b[?1004l"));
}
