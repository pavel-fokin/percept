use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
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
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            chat.textarea.insert_newline();
            Ok(false)
        }
        (KeyCode::PageUp, _) => {
            chat.scroll_up(chat.page_height);
            Ok(false)
        }
        (KeyCode::PageDown, _) => {
            chat.scroll_down(chat.page_height);
            Ok(false)
        }
        (KeyCode::Home, _) => {
            chat.scroll_to_top();
            Ok(false)
        }
        (KeyCode::End, _) => {
            chat.scroll_to_bottom();
            Ok(false)
        }
        // Input still types while a reply streams - only sending
        // waits, so what's typed is sent once the reply lands.
        (KeyCode::Enter, _) if chat.app.is_replying() => Ok(false),
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

/// Scroll the transcript in small steps. Mouse capture is enabled by
/// main, because it owns the terminal rather than the presentation.
pub fn handle_mouse(chat: &mut Chat, mouse: MouseEvent) {
    const WHEEL_LINES: u16 = 3;

    match mouse.kind {
        MouseEventKind::ScrollUp => chat.scroll_up(WHEEL_LINES),
        MouseEventKind::ScrollDown => chat.scroll_down(WHEEL_LINES),
        _ => {}
    }
}

/// Applies one stream event. Whatever the model managed to say is real
/// and commits; a failure is only shown.
pub fn handle_stream(
    chat: &mut Chat,
    event: StreamEvent,
) -> Result<(), Box<dyn std::error::Error>> {
    match event {
        StreamEvent::Chunk(chunk) => chat.app.append_chunk(chunk),
        StreamEvent::Ended(error) => {
            chat.app.end_stream()?;
            chat.error = error;
        }
    }
    Ok(())
}

/// Sends the user's message immediately (visible right away), then
/// spawns a task draining the reply stream, forwarding each chunk and
/// then Ended over reply_tx. An `Err` item ends the turn early, and
/// carries its own words into Ended.
fn submit(
    chat: &mut Chat,
    reply_tx: &UnboundedSender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = chat.textarea.lines().join("\n").trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    chat.textarea.clear();
    chat.error = None;

    let mut stream = chat.app.submit(text)?;
    let reply_tx = reply_tx.clone();
    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    let _ = reply_tx.send(StreamEvent::Chunk(chunk));
                }
                Err(err) => {
                    let _ = reply_tx.send(StreamEvent::Ended(Some(err.to_string())));
                    return;
                }
            }
        }
        let _ = reply_tx.send(StreamEvent::Ended(None));
    });
    Ok(())
}
