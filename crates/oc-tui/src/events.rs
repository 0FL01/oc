//! Key mapping for the minimal TUI.
//!
//! Real terminal bytes arrive via Crossterm; tests inject `KeyAction`
//! directly so no PTY device is needed in unit scope (full PTY
//! qualification stays T26).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    /// Exit the TUI.
    Quit,
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
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(KeyAction::Char(c))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyAction, map_key};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
}
