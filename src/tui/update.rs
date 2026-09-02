use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use std::sync::Arc;

use super::{Chat, StreamEvent};
use crate::app::ToolStep;
use crate::percept::{Chunk, ReplyStream, Tool};

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
/// and commits; a failure is only shown. A tool call is not appended:
/// `App` decides what the call means, and this only carries the
/// decision out - run the tool off-thread, drain the next stream, or
/// stop.
pub fn handle_stream(
    chat: &mut Chat,
    event: StreamEvent,
    reply_tx: &UnboundedSender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    match event {
        StreamEvent::Chunk(Chunk::ToolCall { tool, arguments }) => {
            match chat.app.begin_tool(&tool, arguments)? {
                ToolStep::Run(run, arguments) => spawn_tool(run, arguments, reply_tx.clone()),
                ToolStep::Continue(stream) => spawn_drain(stream, reply_tx.clone()),
                ToolStep::Stop => {}
            }
        }
        StreamEvent::ToolResult(output) => {
            let stream = chat.app.finish_tool(output)?;
            spawn_drain(stream, reply_tx.clone());
        }
        StreamEvent::Chunk(chunk) => chat.app.append_chunk(chunk),
        StreamEvent::Ended(error) => {
            chat.app.end_stream()?;
            chat.error = error;
        }
    }
    Ok(())
}

/// Runs one tool on the blocking pool, off the single-threaded runtime,
/// so a full-log scan never freezes the UI. Its output comes back as a
/// `ToolResult` event.
fn spawn_tool(tool: Arc<dyn Tool>, arguments: String, reply_tx: UnboundedSender<StreamEvent>) {
    tokio::spawn(async move {
        let output = tokio::task::spawn_blocking(move || {
            tool.run(&arguments).unwrap_or_else(|err| err.to_string())
        })
        .await
        .unwrap_or_else(|_| "the tool panicked".to_string());
        let _ = reply_tx.send(StreamEvent::ToolResult(output));
    });
}

/// Drains one reply stream on its own task, forwarding each chunk over
/// `reply_tx`. A tool call is the last chunk of its stream - forward it
/// and stop, no `Ended`, because the turn continues with `resume`. An
/// `Err` item ends the turn early and carries its own words into
/// `Ended`.
fn spawn_drain(mut stream: ReplyStream, reply_tx: UnboundedSender<StreamEvent>) {
    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    let done = matches!(chunk, Chunk::ToolCall { .. });
                    let _ = reply_tx.send(StreamEvent::Chunk(chunk));
                    if done {
                        return;
                    }
                }
                Err(err) => {
                    let _ = reply_tx.send(StreamEvent::Ended(Some(err.to_string())));
                    return;
                }
            }
        }
        let _ = reply_tx.send(StreamEvent::Ended(None));
    });
}

/// Sends the user's message immediately (visible right away), then
/// drains the reply stream on a task.
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

    let stream = chat.app.submit(text)?;
    spawn_drain(stream, reply_tx.clone());
    Ok(())
}
