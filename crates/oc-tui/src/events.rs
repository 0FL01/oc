//! Event mapping for the minimal TUI.
//!
//! Real terminal events arrive via Crossterm; tests inject `KeyAction`
//! directly so no PTY device is needed in unit scope (full PTY
//! qualification stays T26). Pastes are accepted as bounded text so a large
//! terminal paste can never grow the view state without limit.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

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
}

/// Map a Crossterm key event to an action.
///
/// `Esc` cancels a stream (or quits when idle — resolved by `TuiState`);
/// `Ctrl-C`/`Ctrl-D` always quit; `/quit` typed at idle also quits.
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
        (KeyCode::Esc, _) => Some(KeyAction::Cancel),
        (KeyCode::Enter, _) => Some(KeyAction::Enter),
        (KeyCode::Backspace, _) => Some(KeyAction::Backspace),
        (KeyCode::Up, _) => Some(KeyAction::Up),
        (KeyCode::Down, _) => Some(KeyAction::Down),
        (KeyCode::Left, _) => Some(KeyAction::Left),
        (KeyCode::Right, _) => Some(KeyAction::Right),
        (KeyCode::PageUp, _) => Some(KeyAction::PageUp),
        (KeyCode::PageDown, _) => Some(KeyAction::PageDown),
        (KeyCode::Home, _) => Some(KeyAction::Home),
        (KeyCode::End, _) => Some(KeyAction::End),
        (KeyCode::Char(c), m) if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
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
