use std::io;
use std::sync::Arc;

use crossterm::event::{Event as CtEvent, EventStream, KeyEventKind};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

mod app;
mod percept;
mod providers;
mod tui;

use app::Conversation;
use providers::Stub;
use tui::Chat;

async fn run(terminal: &mut ratatui::DefaultTerminal, chat: &mut Chat<'_>) -> io::Result<()> {
    let mut term_events = EventStream::new();
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<String>();

    loop {
        terminal.draw(|frame| tui::draw(frame, chat))?;

        tokio::select! {
            Some(Ok(event)) = term_events.next() => {
                if let CtEvent::Key(key) = event {
                    if key.kind == KeyEventKind::Press
                        && tui::handle_key(chat, key, &reply_tx)
                    {
                        return Ok(());
                    }
                }
            }
            Some(reply) = reply_rx.recv() => {
                chat.conversation.append_reply(reply);
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let conversation = Conversation::new(Arc::new(Stub));
    let mut chat = Chat::new(Box::new(conversation));
    let result = run(&mut terminal, &mut chat).await;
    ratatui::restore();
    result
}
