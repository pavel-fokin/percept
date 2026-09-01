use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use super::{Chat, StreamEvent};

/// Ends the reply when the stream yields an `Err`. Chunks already
/// shown stand, so a reply that broke mid-sentence keeps what arrived.
const REPLY_FAILED: &str = "Sorry, something went wrong.";

/// Handle one key press. Returns true if the app should quit. Errs if
/// submit couldn't append its event to the log - see AppService::submit.
pub fn handle_key(
    chat: &mut Chat,
    key: KeyEvent,
    reply_tx: &UnboundedSender<StreamEvent>,
) -> Result<bool, Box<dyn std::error::Error>> {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Ok(true),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Ok(true),
        (KeyCode::Enter, _) => {
            submit(chat, reply_tx)?;
            Ok(false)
        }
        _ => {
            chat.textarea.input(key);
            Ok(false)
        }
    }
}

/// Sends the user's message immediately (visible right away), then
/// spawns a task draining the reply stream, forwarding each chunk (and
/// a final Done once the stream is exhausted) over reply_tx. An `Err`
/// item ends the reply with a fixed message rather than the chunk it
/// carries.
fn submit(
    chat: &mut Chat,
    reply_tx: &UnboundedSender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = chat.textarea.lines()[0].trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    chat.textarea.clear();

    let mut stream = chat.app.submit(text)?;
    let reply_tx = reply_tx.clone();
    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    let _ = reply_tx.send(StreamEvent::Chunk(chunk));
                }
                Err(_) => {
                    let _ = reply_tx.send(StreamEvent::Chunk(REPLY_FAILED.to_string()));
                    break;
                }
            }
        }
        let _ = reply_tx.send(StreamEvent::Done);
    });
    Ok(())
}
