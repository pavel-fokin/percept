use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;

/// Handle one key press. Returns true if the app should quit.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
        (KeyCode::Enter, _) => {
            app.submit();
            false
        }
        _ => {
            app.textarea.input(key);
            false
        }
    }
}
