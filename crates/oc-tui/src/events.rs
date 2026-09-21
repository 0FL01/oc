//! Event mapping for the minimal TUI.
//!
//! Real terminal events arrive via Crossterm; tests inject `KeyAction`
//! directly so no PTY device is needed in unit scope (full PTY
//! qualification stays T26). Pastes are accepted as bounded text so a large
//! terminal paste can never grow the view state without limit.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

/// Max paste bytes accepted.
pub const PASTE_MAX: usize = 64 * 1024;

/// Minimal actions the chat view understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
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
    /// Move the panel cursor left (variant cycle in the model picker).
    Left,
    /// Move the panel cursor right (variant cycle in the model picker).
    Right,
    /// Exit the TUI.
    Quit,
}

/// One UI-level event: a mapped key, a bounded paste or a resize hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// Key action.
    Key(KeyAction),
    /// Bracketed paste text (bounded to [`PASTE_MAX`], char-boundary safe).
    Paste(String),
    /// Terminal was resized; the next frame re-reads the size.
    Resize,
}

/// Map a Crossterm key event to an action.
///
/// `Esc` cancels a stream (or quits when idle — resolved by `TuiState`);
/// `Ctrl-C`/`Ctrl-D` always quit; `/quit` typed at idle also quits.
pub fn map_key(event: KeyEvent) -> Option<KeyAction> {
    match (event.code, event.modifiers) {
        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => Some(KeyAction::Quit),
        (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => Some(KeyAction::Quit),
        (KeyCode::Esc, _) => Some(KeyAction::Cancel),
        (KeyCode::Enter, _) => Some(KeyAction::Enter),
        (KeyCode::Backspace, _) => Some(KeyAction::Backspace),
        (KeyCode::Up, _) => Some(KeyAction::Up),
        (KeyCode::Down, _) => Some(KeyAction::Down),
        (KeyCode::Left, _) => Some(KeyAction::Left),
        (KeyCode::Right, _) => Some(KeyAction::Right),
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(KeyAction::Char(c))
        }
        _ => None,
    }
}

/// Map a Crossterm terminal event to a UI event.
pub fn map_event(event: Event) -> Option<UiEvent> {
    match event {
        Event::Key(key) => map_key(key).map(UiEvent::Key),
        Event::Paste(text) => Some(UiEvent::Paste(
            crate::truncate_utf8(&text, PASTE_MAX).to_string(),
        )),
        Event::Resize(_, _) => Some(UiEvent::Resize),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyAction, PASTE_MAX, UiEvent, map_event, map_key};
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
            Some(KeyAction::Quit)
        );
        assert_eq!(map_key(key(KeyCode::Char('a'))), Some(KeyAction::Char('a')));
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
    fn paste_is_bounded_on_char_boundary() {
        let text = "ж".repeat(PASTE_MAX);
        let Some(UiEvent::Paste(bounded)) = map_event(Event::Paste(text)) else {
            panic!("paste must map");
        };
        assert!(bounded.len() <= PASTE_MAX);
        assert!(bounded.chars().all(|c| c == 'ж'));
        assert_eq!(
            map_event(Event::Paste(String::new())),
            Some(UiEvent::Paste(String::new()))
        );
    }
}
