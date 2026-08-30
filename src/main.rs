use std::sync::Arc;

use crossterm::event::{Event as CtEvent, EventStream, KeyEventKind};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

mod app;
mod percept;
mod providers;
mod shared;
mod store;
mod tui;

use app::Conversation;
use providers::Stub;
use store::Jsonl;
use tui::{Chat, StreamEvent};

/// Where the event log lives: `percept.jsonl` in the working directory,
/// so the transcript follows wherever the app is launched from.
const LOG_PATH: &str = "percept.jsonl";

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    chat: &mut Chat<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut term_events = EventStream::new();
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<StreamEvent>();

    loop {
        terminal.draw(|frame| tui::draw(frame, chat))?;

        let mut quit = false;
        tokio::select! {
            Some(Ok(event)) = term_events.next() => {
                if let CtEvent::Key(key) = event {
                    if key.kind == KeyEventKind::Press
                        && tui::handle_key(chat, key, &reply_tx)?
                    {
                        quit = true;
                    }
                }
            }
            Some(event) = reply_rx.recv() => apply(chat, event)?,
        }

        if quit {
            // A reply that finished while the quit key was in flight is
            // still queued: select! picks at random between the two
            // ready branches, so the keypress can win. Commit it rather
            // than leaving the log with a prompt and no answer.
            while let Ok(event) = reply_rx.try_recv() {
                apply(chat, event)?;
            }
            return Ok(());
        }
    }
}

fn apply(chat: &mut Chat<'_>, event: StreamEvent) -> Result<(), Box<dyn std::error::Error>> {
    match event {
        StreamEvent::Chunk(chunk) => chat.conversation.append_chunk(chunk),
        StreamEvent::Done => chat.conversation.end_stream()?,
    }
    Ok(())
}

async fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Arc::new(Jsonl::open(LOG_PATH)?);
    let conversation = Conversation::new(Arc::new(Stub), log)?;

    let mut terminal = ratatui::init();
    let mut chat = Chat::new(Box::new(conversation));
    let result = run(&mut terminal, &mut chat).await;
    ratatui::restore();
    result
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = try_main().await {
        eprintln!("percept: {err}");
        std::process::exit(1);
    }
}
