use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Chat;

/// Handle one key press. Returns true if the app should quit.
pub fn handle_key(chat: &mut Chat, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
        (KeyCode::Enter, _) => {
            submit(chat);
            false
        }
        _ => {
            chat.textarea.input(key);
            false
        }
    }
}

/// Sends the pending input to the application layer, then resets the
/// input. Bails without resetting if the app layer errors, same as Go.
fn submit(chat: &mut Chat) {
    let text = chat.textarea.lines()[0].trim().to_string();
    if text.is_empty() {
        return;
    }
    if chat.conversation.submit(text).is_ok() {
        chat.textarea.clear();
    }
}
