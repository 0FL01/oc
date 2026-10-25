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

/// Frames of a scrolling, styled document with a fixed side panel and a
/// footer that sometimes changes; `None` clears the terminal.
fn scroll_frames() -> Vec<Option<(usize, usize, usize)>> {
    let mut frames = Vec::new();
    let mut offset = 0usize;
    let mut step = |delta: isize, footer: usize, side: usize, frames: &mut Vec<_>| {
        offset = offset.saturating_add_signed(delta).min(180);
        frames.push(Some((offset, footer, side)));
    };
    for _ in 0..20 {
        step(1, 0, 0, &mut frames);
    }
    for _ in 0..8 {
        step(3, 0, 0, &mut frames);
    }
    for _ in 0..8 {
        step(-2, 0, 0, &mut frames);
    }
    for footer in 1..4 {
        step(0, footer, 0, &mut frames);
    }
    for side in 1..4 {
        step(1, 3, side, &mut frames);
    }
    for _ in 0..4 {
        step(12, 3, 3, &mut frames);
    }
    frames.push(None);
    for delta in [5, -7, 1, 1, -1, 40, -3, 0, 2] {
        step(delta, 4, 4, &mut frames);
    }
    frames
}

fn scroll_document(row: usize) -> ratatui::text::Line<'static> {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    if row.is_multiple_of(11) {
        return Line::default();
    }
    let bg = match row % 5 {
        0 => Color::Rgb(32, 64, 32),
        1 => Color::Rgb(64, 32, 32),
        _ => Color::Rgb(24, 24, 32),
    };
    let fg = Color::Rgb((row * 37 % 256) as u8, 200, (row * 11 % 256) as u8);
    let mut spans = vec![
        Span::styled(format!("{row:>4} "), Style::default().fg(fg).bg(bg)),
        Span::styled(
            format!("let value_{row} = compute({row});"),
            Style::default()
                .fg(Color::Rgb(220, 220, 220))
                .bg(bg)
                .add_modifier(if row.is_multiple_of(3) {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
    ];
    if row.is_multiple_of(7) {
        spans.push(Span::styled(" 界面", Style::default().bg(bg)));
    }
    Line::from(spans).style(Style::default().bg(bg))
}

fn draw_scroll_frame(frame: &mut ratatui::Frame, (offset, footer, side): (usize, usize, usize)) {
    use ratatui::style::{Color, Style};
    let area = frame.area();
    let body = Rect::new(0, 0, area.width - 12, area.height - 4);
    let lines: Vec<_> = (offset..offset + usize::from(body.height))
        .map(scroll_document)
        .collect();
    frame.render_widget(Paragraph::new(lines), body);
    frame.render_widget(
        Paragraph::new(format!("side {side}\nstatic"))
            .style(Style::default().bg(Color::Rgb(40, 40, 60))),
        Rect::new(body.width, 0, 12, body.height),
    );
    frame.render_widget(
        Paragraph::new(format!("prompt draft {footer}"))
            .style(Style::default().bg(Color::Rgb(50, 50, 50))),
        Rect::new(0, body.height, area.width, 4),
    );
}

fn vt_color(color: ratatui::style::Color) -> vt100::Color {
    match color {
        ratatui::style::Color::Reset => vt100::Color::Default,
        ratatui::style::Color::Rgb(r, g, b) => vt100::Color::Rgb(r, g, b),
        other => panic!("unexpected fixture color {other:?}"),
    }
}

/// Run the frames through `FrameBackend` into a VT parser, asserting after
/// every frame that the emulated screen equals Ratatui's buffer; returns the
/// published bytes per frame.
fn publish_scroll_frames(track: bool) -> Vec<usize> {
    let (width, height) = (48u16, 16u16);
    let recorded = Rc::new(RefCell::new(Recorded::default()));
    let backend = FrameBackend::new(Output {
        recorded: recorded.clone(),
        fragment: usize::MAX,
        fail_at: None,
        fail_flush: false,
    });
    if track {
        backend
            .assume_size(ratatui::layout::Size::new(width, height))
            .unwrap();
    }
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, width, height)),
        },
    )
    .unwrap();
    let mut parser = vt100::Parser::new(height, width, 0);
    let mut sizes = Vec::new();
    for (index, frame) in scroll_frames().into_iter().enumerate() {
        let start = recorded.borrow().bytes.len();
        terminal.backend_mut().begin_frame().unwrap();
        let Some(frame) = frame else {
            // `Terminal::clear` of a full screen, without its cursor query.
            terminal
                .backend_mut()
                .clear_region(ratatui::backend::ClearType::All)
                .unwrap();
            terminal.swap_buffers();
            terminal.backend_mut().finish_frame().unwrap();
            parser.process(&recorded.borrow().bytes[start..]);
            continue;
        };
        let expected = terminal
            .draw(|f| draw_scroll_frame(f, frame))
            .unwrap()
            .buffer
            .clone();
        terminal.backend_mut().finish_frame().unwrap();
        let bytes = recorded.borrow().bytes[start..].to_vec();
        sizes.push(bytes.len());
        parser.process(&bytes);
        let screen = parser.screen();
        for y in 0..height {
            for x in 0..width {
                let cell = &expected[(x, y)];
                let shown = screen.cell(y, x).unwrap();
                if shown.is_wide_continuation() {
                    continue;
                }
                let symbol = if shown.contents().is_empty() {
                    " "
                } else {
                    shown.contents()
                };
                let at = format!("frame {index} {frame:?} cell ({x}, {y})");
                assert_eq!(symbol, cell.symbol(), "{at}");
                assert_eq!(shown.fgcolor(), vt_color(cell.fg), "{at}");
                assert_eq!(shown.bgcolor(), vt_color(cell.bg), "{at}");
                assert_eq!(
                    shown.bold(),
                    cell.modifier.contains(ratatui::style::Modifier::BOLD),
                    "{at}"
                );
            }
        }
    }
    sizes
}

/// Scrolled frames published as terminal scrolls leave exactly Ratatui's
/// cells on an emulated terminal, and cost far fewer bytes than repainting.
#[test]
fn scrolled_frames_publish_as_terminal_scrolls_with_identical_cells() {
    let plain = publish_scroll_frames(false);
    let scrolled = publish_scroll_frames(true);
    let wheel = |sizes: &[usize]| sizes[..20].iter().sum::<usize>();
    assert!(
        wheel(&scrolled) * 3 < wheel(&plain),
        "wheel steps publish {} bytes as scrolls vs {} repainted",
        wheel(&scrolled),
        wheel(&plain)
    );
    assert!(scrolled.iter().sum::<usize>() < plain.iter().sum::<usize>());
}

/// The cursor a frame leaves (shown at a position, or hidden) is what a
/// later partial frame must repeat.
#[test]
fn painted_cursor_follows_the_last_frame() {
    let recorded = Rc::new(RefCell::new(Recorded::default()));
    let backend = FrameBackend::new(Output {
        recorded,
        fragment: usize::MAX,
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
    for (cursor, expected) in [
        (Some((4, 2)), Some(ratatui::layout::Position::new(4, 2))),
        (None, None),
        (Some((1, 3)), Some(ratatui::layout::Position::new(1, 3))),
    ] {
        terminal.backend_mut().begin_frame().unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new("prompt"), frame.area());
                if let Some(cursor) = cursor {
                    frame.set_cursor_position(cursor);
                }
            })
            .unwrap();
        terminal.backend_mut().finish_frame().unwrap();
        assert_eq!(terminal.backend().painted_cursor(), expected);
    }
}

fn assert_shown(parser: &vt100::Parser, expected: &ratatui::buffer::Buffer, at: &str) {
    let screen = parser.screen();
    for y in 0..expected.area.height {
        for x in 0..expected.area.width {
            let cell = &expected[(x, y)];
            let shown = screen.cell(y, x).unwrap();
            if shown.is_wide_continuation() {
                continue;
            }
            let symbol = if shown.contents().is_empty() {
                " "
            } else {
                shown.contents()
            };
            let at = format!("{at} cell ({x}, {y})");
            assert_eq!(symbol, cell.symbol(), "{at}");
            assert_eq!(shown.fgcolor(), vt_color(cell.fg), "{at}");
            assert_eq!(shown.bgcolor(), vt_color(cell.bg), "{at}");
        }
    }
}

/// Regions published outside Ratatui (strip-only frames) leave the
/// emulated terminal equal to the region, and the next Ratatui frames,
/// whose diff is relative to Ratatui's stale previous buffer, still leave it
/// exactly equal to Ratatui's buffer, including a frame that keeps the
/// strip's old content and a scrolled one; the cursor is restored.
#[test]
fn region_frames_outside_ratatui_reconcile_with_the_next_frames() {
    use ratatui::style::{Color, Style};
    let (width, height) = (48u16, 16u16);
    let recorded = Rc::new(RefCell::new(Recorded::default()));
    let backend = FrameBackend::new(Output {
        recorded: recorded.clone(),
        fragment: usize::MAX,
        fail_at: None,
        fail_flush: false,
    });
    backend
        .assume_size(ratatui::layout::Size::new(width, height))
        .unwrap();
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, width, height)),
        },
    )
    .unwrap();
    let mut parser = vt100::Parser::new(height, width, 0);
    let publish = |terminal: &mut Terminal<FrameBackend<Output>>,
                   parser: &mut vt100::Parser,
                   draw: &mut dyn FnMut(&mut Terminal<FrameBackend<Output>>)| {
        let start = recorded.borrow().bytes.len();
        terminal.backend_mut().begin_frame().unwrap();
        draw(terminal);
        terminal.backend_mut().finish_frame().unwrap();
        parser.process(&recorded.borrow().bytes[start..]);
    };
    let strip = |label: &str| {
        let mut region = ratatui::buffer::Buffer::empty(Rect::new(0, 0, width, 1));
        region.set_string(
            0,
            0,
            format!("{label} 界面 strip"),
            Style::default()
                .fg(Color::Rgb(200, 100, 50))
                .bg(Color::Rgb(20, 20, 30)),
        );
        region
    };
    let full = |offset: usize, header: &'static str| {
        move |frame: &mut ratatui::Frame| {
            frame.render_widget(
                Paragraph::new(header.to_string())
                    .style(Style::default().bg(Color::Rgb(20, 20, 30))),
                Rect::new(0, 0, width, 1),
            );
            let lines: Vec<_> = (offset..offset + 14).map(scroll_document).collect();
            frame.render_widget(Paragraph::new(lines), Rect::new(0, 1, width, 14));
            frame.set_cursor_position((3, 15));
        }
    };
    let mut expected = None;
    publish(&mut terminal, &mut parser, &mut |terminal| {
        expected = Some(terminal.draw(full(0, "header A")).unwrap().buffer.clone());
    });
    assert_shown(&parser, expected.as_ref().unwrap(), "first full frame");
    for (index, label) in ["sweep one", "sweep 二"].into_iter().enumerate() {
        let region = strip(label);
        publish(&mut terminal, &mut parser, &mut |terminal| {
            let cursor = terminal.backend().painted_cursor();
            assert!(terminal.backend_mut().draw_region(&region, cursor).unwrap());
        });
        let mut shown = expected.clone().unwrap();
        for at in region.area.positions() {
            shown[at] = region[at].clone();
        }
        assert_shown(&parser, &shown, &format!("region {index}"));
        assert_eq!(parser.screen().cursor_position(), (15, 3));
        assert!(!parser.screen().hide_cursor());
    }
    for (step, (offset, header)) in [(0, "header A"), (1, "header A"), (4, "header B")]
        .into_iter()
        .enumerate()
    {
        publish(&mut terminal, &mut parser, &mut |terminal| {
            expected = Some(terminal.draw(full(offset, header)).unwrap().buffer.clone());
        });
        assert_shown(
            &parser,
            expected.as_ref().unwrap(),
            &format!("full frame {step}"),
        );
    }
}
