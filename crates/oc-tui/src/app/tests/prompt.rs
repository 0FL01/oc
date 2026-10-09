use super::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};

fn pointer(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

#[tokio::test]
async fn vis12_prompt_click_moves_editor_before_typing_on_home_and_session() {
    for home in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = if home {
            TuiState::new_home(app)
        } else {
            TuiState::new(app, sid("caret"))
        };
        if !home {
            state.status = TuiStatus::Streaming;
        }
        state.restore_prompt("Проведи RECON, жду план".into());
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let area = Rect::new(0, 0, 120, 40);
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let (x, y) = (0..40)
            .flat_map(|y| (0..116).map(move |x| (x, y)))
            .find(|&(x, y)| {
                ["R", "E", "C", "O", "N"]
                    .iter()
                    .enumerate()
                    .all(|(i, expected)| buffer[(x + i as u16, y)].symbol() == *expected)
            })
            .unwrap();
        let old = state.editor.cursor;
        state.handle_mouse(pointer(MouseEventKind::Moved, x + 3, y), area);
        assert_eq!(state.editor.cursor, old, "hover has no caret ownership");
        let outcome = state.handle_mouse(
            pointer(MouseEventKind::Down(MouseButton::Left), x + 3, y),
            area,
        );
        assert!(outcome.intent.is_none());
        assert_eq!(state.input(), "Проведи RECON, жду план");
        assert_eq!(state.editor.cursor, "Проведи REC".len());
        state.handle_mouse(
            pointer(MouseEventKind::Up(MouseButton::Left), x + 3, y),
            area,
        );
        state.handle_key(KeyAction::Char('X')).await;
        assert_eq!(state.input(), "Проведи RECXON, жду план");
        assert!(
            inbox.try_recv().is_err(),
            "navigation/edit is not a request"
        );
    }
}

#[tokio::test]
async fn vis12_painted_prompt_stops_cover_unicode_wrap_blank_and_clipped_rows() {
    use unicode_segmentation::UnicodeSegmentation;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let area = Rect::new(0, 0, 120, 40);
    let unicode = "α界e\u{301}👩\u{200d}💻 tail";
    // The second wide cell, combining sequence and ZWJ emoji all resolve to
    // legal raw stops from the same painted editor projection.
    for (column, offset) in [(2, "α".len()), (3, "α界".len()), (5, "α界e\u{301}".len())] {
        state.restore_prompt(unicode.into());
        state.handle_key(KeyAction::SelectLeft).await;
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let rect = state.painted_prompt.borrow().as_ref().unwrap().input;
        state.handle_mouse(
            pointer(
                MouseEventKind::Down(MouseButton::Left),
                rect.x + column,
                rect.y,
            ),
            area,
        );
        assert_eq!(state.editor.cursor, offset);
        assert!(
            state.editor.selected().is_none(),
            "ordinary Down clears keyboard selection"
        );
        assert!(unicode.grapheme_indices(true).any(|(at, _)| at == offset));
        state.handle_key(KeyAction::Char('X')).await;
        let mut expected = unicode.to_owned();
        expected.insert(offset, 'X');
        assert_eq!(state.input(), expected);
        state.handle_key(KeyAction::Backspace).await;
        assert_eq!(state.input(), unicode);
        state.handle_paste("Ω");
        expected = unicode.into();
        expected.insert(offset, 'Ω');
        assert_eq!(state.input(), expected);
        state.handle_key(KeyAction::Undo).await;
        state.handle_key(KeyAction::Delete).await;
        let end = unicode[offset..].graphemes(true).next().unwrap().len() + offset;
        expected = unicode.into();
        expected.replace_range(offset..end, "");
        assert_eq!(state.input(), expected);
    }
    for (text, row, column, offset) in [
        (format!("{} LONGWORD", "a".repeat(65)), 0, 69, 65),
        ("first\n\nlast".into(), 1, 30, 6),
        ("tail".into(), 0, 69, 4),
    ] {
        state.restore_prompt(text.clone());
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let rect = state.painted_prompt.borrow().as_ref().unwrap().input;
        state.handle_mouse(
            pointer(
                MouseEventKind::Down(MouseButton::Left),
                rect.x + column,
                rect.y + row,
            ),
            area,
        );
        assert_eq!(state.input(), text);
        assert_eq!(state.editor.cursor, offset);
        state.handle_key(KeyAction::Char('X')).await;
        let mut expected = text;
        expected.insert(offset, 'X');
        assert_eq!(state.input(), expected);
    }
    let text = (0..24)
        .map(|n| format!("line{n:02} 界e\u{301}👩\u{200d}💻"))
        .collect::<Vec<_>>()
        .join("\n");
    state.restore_prompt(text.clone());
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let rect = state.painted_prompt.borrow().as_ref().unwrap().input;
    let shown = (rect.x..rect.right())
        .map(|x| terminal.backend().buffer()[(x, rect.y)].symbol())
        .collect::<String>();
    let first_line = shown.split_whitespace().next().unwrap();
    let offset = text.find(first_line).unwrap() + 2;
    assert!(
        offset > 2,
        "the input viewport is scrolled past the raw first line"
    );
    // Old frame coordinates must not apply after resize, even if in bounds.
    state.handle_mouse(
        pointer(MouseEventKind::Down(MouseButton::Left), rect.x + 2, rect.y),
        Rect::new(0, 0, 119, 40),
    );
    assert_eq!(state.editor.cursor, text.len());
    state.handle_mouse(
        pointer(MouseEventKind::Down(MouseButton::Left), rect.x + 2, rect.y),
        area,
    );
    assert_eq!(state.editor.cursor, offset);
    state.handle_key(KeyAction::Char('X')).await;
    let mut expected = text;
    expected.insert(offset, 'X');
    assert_eq!(state.input(), expected);
    // Higher-priority owners may change after this paint. The old valid text
    // rectangle must still never edit beneath them or under a modified click.
    type Owner = fn(&mut TuiState, bool);
    let owners: [Owner; 3] = [
        |s, open| s.composer.active = open.then_some(crate::composer::Tab::Subagents),
        |s, open| s.composer.active = open.then_some(crate::composer::Tab::Shell),
        |s, open| s.composer.active = open.then_some(crate::composer::Tab::Terminals),
    ];
    for owner in owners {
        state.restore_prompt("guard RECON".into());
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let rect = state.painted_prompt.borrow().as_ref().unwrap().input;
        owner(&mut state, true);
        state.handle_mouse(
            pointer(MouseEventKind::Down(MouseButton::Left), rect.x + 7, rect.y),
            area,
        );
        assert_eq!(state.editor.cursor, "guard RECON".len());
        assert_eq!(state.input(), "guard RECON");
        owner(&mut state, false);
    }
    state.restore_prompt("guard RECON".into());
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let rect = state.painted_prompt.borrow().as_ref().unwrap().input;
    for modifiers in [
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
        KeyModifiers::SHIFT,
    ] {
        let mut event = pointer(MouseEventKind::Down(MouseButton::Left), rect.x + 7, rect.y);
        event.modifiers = modifiers;
        state.handle_mouse(event, area);
        assert_eq!(state.editor.cursor, "guard RECON".len());
    }
    assert!(inbox.try_recv().is_err());
}
