use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use super::{Chat, StreamEvent};

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
        // Input still types while a reply streams - only sending
        // waits, so what's typed is sent once the reply lands.
        (KeyCode::Enter, _) if chat.replying => Ok(false),
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
/// item ends the reply as Failed instead - whatever text already
/// arrived still commits, but the error itself never does.
fn submit(
    chat: &mut Chat,
    reply_tx: &UnboundedSender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = chat.textarea.lines()[0].trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    chat.textarea.clear();
    chat.error = None;
    chat.replying = true;

    let mut stream = chat.app.submit(text)?;
    let reply_tx = reply_tx.clone();
    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    let _ = reply_tx.send(StreamEvent::Chunk(chunk));
                }
                Err(err) => {
                    let _ = reply_tx.send(StreamEvent::Failed(err.to_string()));
                    return;
                }
            }
        }
        let _ = reply_tx.send(StreamEvent::Done);
    });
    Ok(())
}
