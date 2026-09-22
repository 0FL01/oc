//! Event mapping for the minimal TUI.
//!
//! Real terminal events arrive via Crossterm; tests inject `KeyAction`
//! directly so no PTY device is needed in unit scope (full PTY
//! qualification stays T26). Pastes are accepted as bounded text so a large
//! terminal paste can never grow the view state without limit.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};

/// Minimal actions the chat view understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// Open Commands (previous item when a modal owns focus).
    Commands,
    /// Begin the configured upstream default leader chord.
    Leader,
    /// Ctrl+C: clear search or dismiss a modal; exit at the root.
    Interrupt,
    /// Open the native agent selector.
    Agents,
    /// Modal page navigation.
    PageUp,
    /// Modal page navigation.
    PageDown,
    /// Modal first item.
    Home,
    /// Modal last item.
    End,
    /// Printable input.
    Char(char),
    /// Insert a line break without submitting.
    Newline,
    /// Forward delete (or selected range).
    Delete,
    /// Ctrl+D: editor delete when nonempty, root exit otherwise.
    DeleteOrQuit,
    /// Move/delete one word.
    WordLeft,
    WordRight,
    WordBackspace,
    WordDelete,
    /// Extend selection over a grapheme or word.
    SelectLeft,
    SelectRight,
    SelectWordLeft,
    SelectWordRight,
    SelectUp,
    SelectDown,
    SelectHome,
    SelectEnd,
    Undo,
    Redo,
    /// Backspace.
    Backspace,
    /// Submit input buffer.
    Enter,
    /// Cancel active turn.
    Cancel,
    /// Scroll viewport.
    Up,
    /// Scroll viewport.
    Down,
    /// Left arrow (reserved in selectors).
    Left,
    /// Right arrow (reserved in selectors).
    Right,
    /// Exit the TUI.
    Quit,
}

/// One UI-level event: a mapped key, a bounded paste or a resize hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// Key action.
    Key(KeyAction),
    /// Bracketed paste text, passed through unchanged: the input budget is
    /// enforced (and reported) by `TuiState::handle_paste`.
    Paste(String),
    /// Terminal was resized; the next frame re-reads the size.
    Resize,
    /// Terminal-reported pointer position/button/scroll (zero-based cells).
    Mouse(MouseEvent),
}

/// Map a Crossterm key event to an action.
///
/// `Esc` cancels a stream (or quits when idle — resolved by `TuiState`);
/// Root Ctrl+C exits; Ctrl+D deletes at the editor or exits at an empty root.
pub fn map_key(event: KeyEvent) -> Option<KeyAction> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    if event.kind == KeyEventKind::Repeat
        && (matches!(event.code, KeyCode::Enter | KeyCode::Esc)
            || event.modifiers.contains(KeyModifiers::CONTROL))
    {
        return None;
    }
    // Dialog bindings are resolved by the focused dialog, before any global
    // shortcut. In particular Ctrl+P moves the selection there.
    if event.code == KeyCode::Char('j') && event.modifiers == KeyModifiers::CONTROL {
        return Some(KeyAction::Newline);
    }
    if event.code == KeyCode::Char('d') && event.modifiers == KeyModifiers::CONTROL {
        return Some(KeyAction::DeleteOrQuit);
    }
    if let Some(action) = crate::commands::direct(event) {
        return match action {
            crate::commands::CommandAction::OpenCommands => Some(KeyAction::Commands),
            crate::commands::CommandAction::OpenAgents => Some(KeyAction::Agents),
            crate::commands::CommandAction::Quit if event.code == KeyCode::Char('c') => {
                Some(KeyAction::Interrupt)
            }
            crate::commands::CommandAction::Quit => Some(KeyAction::Quit),
            _ => None,
        };
    }
    match (event.code, event.modifiers) {
        (KeyCode::Char('x'), KeyModifiers::CONTROL) => Some(KeyAction::Leader),
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => Some(KeyAction::Down),
        (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(KeyAction::Left),
        (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(KeyAction::Right),
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => Some(KeyAction::Home),
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => Some(KeyAction::End),
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => Some(KeyAction::WordBackspace),
        (KeyCode::Char('-'), KeyModifiers::CONTROL) => Some(KeyAction::Undo),
        (KeyCode::Char('.'), KeyModifiers::CONTROL) => Some(KeyAction::Redo),
        (KeyCode::Char('b'), KeyModifiers::ALT) => Some(KeyAction::WordLeft),
        (KeyCode::Char('f'), KeyModifiers::ALT) => Some(KeyAction::WordRight),
        (KeyCode::Char('d'), KeyModifiers::ALT) => Some(KeyAction::WordDelete),
        (KeyCode::Esc, _) => Some(KeyAction::Cancel),
        (KeyCode::Enter, KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(KeyAction::Newline)
        }
        (KeyCode::Enter, KeyModifiers::NONE) => Some(KeyAction::Enter),
        (KeyCode::Backspace, KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(KeyAction::WordBackspace)
        }
        (KeyCode::Backspace, _) => Some(KeyAction::Backspace),
        (KeyCode::Delete, KeyModifiers::CONTROL | KeyModifiers::ALT) => Some(KeyAction::WordDelete),
        (KeyCode::Delete, _) => Some(KeyAction::Delete),
        (KeyCode::Left, KeyModifiers::SHIFT) => Some(KeyAction::SelectLeft),
        (KeyCode::Right, KeyModifiers::SHIFT) => Some(KeyAction::SelectRight),
        (KeyCode::Up, KeyModifiers::SHIFT) => Some(KeyAction::SelectUp),
        (KeyCode::Down, KeyModifiers::SHIFT) => Some(KeyAction::SelectDown),
        (KeyCode::Up, _) => Some(KeyAction::Up),
        (KeyCode::Down, _) => Some(KeyAction::Down),
        (KeyCode::Left, KeyModifiers::ALT | KeyModifiers::CONTROL) => Some(KeyAction::WordLeft),
        (KeyCode::Right, KeyModifiers::ALT | KeyModifiers::CONTROL) => Some(KeyAction::WordRight),
        (KeyCode::Left, m)
            if m.contains(KeyModifiers::SHIFT)
                && m.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
        {
            Some(KeyAction::SelectWordLeft)
        }
        (KeyCode::Right, m)
            if m.contains(KeyModifiers::SHIFT)
                && m.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
        {
            Some(KeyAction::SelectWordRight)
        }
        (KeyCode::Left, _) => Some(KeyAction::Left),
        (KeyCode::Right, _) => Some(KeyAction::Right),
        (KeyCode::PageUp, _) => Some(KeyAction::PageUp),
        (KeyCode::PageDown, _) => Some(KeyAction::PageDown),
        (KeyCode::Home, KeyModifiers::SHIFT) => Some(KeyAction::SelectHome),
        (KeyCode::End, KeyModifiers::SHIFT) => Some(KeyAction::SelectEnd),
        (KeyCode::Home, _) => Some(KeyAction::Home),
        (KeyCode::End, _) => Some(KeyAction::End),
        (KeyCode::Char(c), m)
            if !c.is_control() && !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(KeyAction::Char(c))
        }
        _ => None,
    }
}

/// Map a Crossterm terminal event to a UI event.
pub fn map_event(event: Event) -> Option<UiEvent> {
    match event {
        Event::Key(key) => map_key(key).map(UiEvent::Key),
        Event::Paste(text) => Some(UiEvent::Paste(text)),
        Event::Resize(_, _) => Some(UiEvent::Resize),
        Event::Mouse(mouse) => Some(UiEvent::Mouse(mouse)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyAction, UiEvent, map_event, map_key};
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn maps_basics() {
        assert_eq!(map_key(key(KeyCode::Enter)), Some(KeyAction::Enter));
        assert_eq!(map_key(key(KeyCode::Esc)), Some(KeyAction::Cancel));
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(KeyAction::Interrupt)
        );
        assert_eq!(map_key(key(KeyCode::Char('a'))), Some(KeyAction::Char('a')));
    }

    #[test]
    fn dialog_routing_rejects_modifier_text_and_release() {
        for modifier in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            assert_eq!(map_key(KeyEvent::new(KeyCode::Char('z'), modifier)), None);
        }
        assert_eq!(
            map_key(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                crossterm::event::KeyEventKind::Release
            )),
            None
        );
    }

    #[test]
    fn v05_modifier_newlines_navigation_and_event_kinds() {
        use crossterm::event::KeyEventKind;
        for modifier in [
            KeyModifiers::SHIFT,
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
        ] {
            assert_eq!(
                map_key(KeyEvent::new(KeyCode::Enter, modifier)),
                Some(KeyAction::Newline)
            );
        }
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
            Some(KeyAction::Newline)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            Some(KeyAction::Commands)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)),
            Some(KeyAction::Leader)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT)),
            Some(KeyAction::SelectLeft)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)),
            Some(KeyAction::WordRight)
        );
        assert_eq!(
            map_key(KeyEvent::new_with_kind(
                KeyCode::Char('я'),
                KeyModifiers::NONE,
                KeyEventKind::Repeat
            )),
            Some(KeyAction::Char('я'))
        );
        assert_eq!(
            map_key(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Repeat
            )),
            None
        );
        assert_eq!(
            map_key(KeyEvent::new_with_kind(
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                KeyEventKind::Release
            )),
            None
        );
    }

    #[test]
    fn map_event_routes_key_paste_resize() {
        assert_eq!(
            map_event(Event::Key(key(KeyCode::Char('x')))),
            Some(UiEvent::Key(KeyAction::Char('x')))
        );
        assert_eq!(
            map_event(Event::Paste("hi".to_string())),
            Some(UiEvent::Paste("hi".to_string()))
        );
        assert_eq!(map_event(Event::Resize(80, 24)), Some(UiEvent::Resize));
        assert_eq!(map_event(Event::FocusGained), None);
    }

    #[test]
    fn paste_passes_through_unbounded() {
        // Bounding happens once, in the input buffer, with a visible note.
        let text = "ж".repeat(200 * 1024);
        let Some(UiEvent::Paste(bounded)) = map_event(Event::Paste(text)) else {
            panic!("paste must map");
        };
        assert_eq!(bounded.len(), 400 * 1024);
        assert!(bounded.chars().all(|c| c == 'ж'));
        assert_eq!(
            map_event(Event::Paste(String::new())),
            Some(UiEvent::Paste(String::new()))
        );
    }
}
