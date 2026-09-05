use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use std::sync::Arc;

use super::{Chat, ModelsMenu, StreamEvent};
use crate::app::{run_tool, ToolStep};
use crate::percept::{Chunk, ModelListing, ReplyStream, Tool, ToolOutput};

/// The one slash command wired up today - exact match, no arguments.
const MODELS_COMMAND: &str = "/models";

/// Handle one key press. Returns true if the app should quit. Errs if
/// submit couldn't append its event to the log - see AppService::submit.
pub fn handle_key(
    chat: &mut Chat,
    key: KeyEvent,
    reply_tx: &UnboundedSender<StreamEvent>,
) -> Result<bool, Box<dyn std::error::Error>> {
    // A hard quit, unlike Esc, which closes the popup instead while
    // it's open - so it has to win over the popup diversion below, not
    // just the ordinary bindings past it.
    if (key.code, key.modifiers) == (KeyCode::Char('c'), KeyModifiers::CONTROL) {
        return Ok(true);
    }
    if chat.models_menu.is_some() {
        handle_models_menu_key(chat, key);
        return Ok(false);
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Ok(true),
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
        // waits, so what's typed is sent once the reply lands. Applies
        // to /models too: a switch can never land mid-turn.
        (KeyCode::Enter, _) if chat.app.is_replying() => Ok(false),
        (KeyCode::Enter, _) if is_models_command(&current_text(chat)) => {
            open_models_menu(chat, reply_tx);
            Ok(false)
        }
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

/// Whether `text`, trimmed, is exactly `/models` - the only slash
/// command today. Anything else starting with `/` is ordinary chat
/// text, so a message that happens to start with `/` is never swallowed.
fn is_models_command(text: &str) -> bool {
    text.trim() == MODELS_COMMAND
}

fn current_text(chat: &Chat) -> String {
    chat.textarea.lines().join("\n")
}

/// Clears the input, opens the popup in its loading state, and kicks
/// off the fetch. The list arrives later as a `ModelsListed` event.
fn open_models_menu(chat: &mut Chat, reply_tx: &UnboundedSender<StreamEvent>) {
    chat.textarea.clear();
    chat.error = None;
    let token = chat.new_models_token();
    chat.models_menu = Some(ModelsMenu::loading(token));
    spawn_models(chat.app.available_models(), token, reply_tx.clone());
}

/// Key handling while the `/models` popup is open. Every other key is
/// swallowed - no typeahead filtering, and Esc closes the popup rather
/// than quitting the app the way it does with no popup open.
fn handle_models_menu_key(chat: &mut Chat, key: KeyEvent) {
    let Some(menu) = chat.models_menu.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Up => menu.move_up(),
        KeyCode::Down => menu.move_down(),
        KeyCode::Enter => {
            if let Some(descriptor) = menu.selected().cloned() {
                match chat.app.set_model(&descriptor) {
                    Ok(()) => {
                        chat.models_menu = None;
                        chat.error = None;
                    }
                    Err(err) => chat.error = Some(err.to_string()),
                }
            }
        }
        KeyCode::Esc => chat.models_menu = None,
        _ => {}
    }
}

/// Scroll the transcript in small steps, or toggle a committed thought
/// clicked on. Mouse capture is enabled by main, because it owns the
/// terminal rather than the presentation.
pub fn handle_mouse(chat: &mut Chat, mouse: MouseEvent) {
    const WHEEL_LINES: u16 = 3;

    match mouse.kind {
        MouseEventKind::ScrollUp => chat.scroll_up(WHEEL_LINES),
        MouseEventKind::ScrollDown => chat.scroll_down(WHEEL_LINES),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(id) = chat.thought_at(mouse.row) {
                chat.toggle_thought(id);
            }
        }
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
            chat.thinking_started = None;
        }
        // The popup may have been closed (Esc) before the fetch landed,
        // or closed and reopened since - either way, a token that
        // doesn't match the open menu's own means this fetch is stale
        // and its result is silently dropped.
        StreamEvent::ModelsListed(token, descriptors) => {
            if let Some(menu) = chat.models_menu.as_mut() {
                if menu.token() == token {
                    menu.populate(descriptors);
                }
            }
        }
    }
    Ok(())
}

/// Runs one tool on the blocking pool, off the single-threaded runtime,
/// so a full-log scan never freezes the UI. Its output comes back as a
/// `ToolResult` event.
fn spawn_tool(tool: Arc<dyn Tool>, arguments: String, reply_tx: UnboundedSender<StreamEvent>) {
    tokio::spawn(async move {
        let output = tokio::task::spawn_blocking(move || run_tool(&*tool, &arguments))
            .await
            .unwrap_or_else(|_| ToolOutput::text("the tool panicked"));
        let _ = reply_tx.send(StreamEvent::ToolResult(output));
    });
}

/// Fetches the model catalog on its own task, so a slow provider never
/// freezes the UI. Its result comes back as a `ModelsListed` event,
/// carrying `token` so a stale fetch can be told from the menu it
/// started for.
fn spawn_models(listing: ModelListing, token: u32, reply_tx: UnboundedSender<StreamEvent>) {
    tokio::spawn(async move {
        let descriptors = listing.await;
        let _ = reply_tx.send(StreamEvent::ModelsListed(token, descriptors));
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
    chat.thinking_started = Some(std::time::Instant::now());

    let stream = chat.app.submit(text)?;
    spawn_drain(stream, reply_tx.clone());
    Ok(())
}

#[cfg(test)]
mod tests;
