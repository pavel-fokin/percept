use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use super::{Chat, StreamEvent};

/// Handle one key press. Returns true if the app should quit.
pub fn handle_key(chat: &mut Chat, key: KeyEvent, reply_tx: &UnboundedSender<StreamEvent>) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
        (KeyCode::Enter, _) => {
            submit(chat, reply_tx);
            false
        }
        _ => {
            chat.textarea.input(key);
            false
        }
    }
}

/// Sends the user's message immediately (visible right away), then
/// spawns the reply thunk on tokio's blocking pool, forwarding each
/// chunk (and a final Done once the thunk returns) over reply_tx.
fn submit(chat: &mut Chat, reply_tx: &UnboundedSender<StreamEvent>) {
    let text = chat.textarea.lines()[0].trim().to_string();
    if text.is_empty() {
        return;
    }
    chat.textarea.clear();

    let stream = chat.conversation.submit(text);
    let reply_tx = reply_tx.clone();
    tokio::task::spawn_blocking(move || {
        let mut on_chunk = |chunk: String| {
            let _ = reply_tx.send(StreamEvent::Chunk(chunk));
        };
        stream(&mut on_chunk);
        let _ = reply_tx.send(StreamEvent::Done);
    });
}
