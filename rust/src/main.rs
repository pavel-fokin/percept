use std::io;
use std::time::Duration;

use crossterm::event::{Event, KeyEventKind};

mod app;
mod percept;
mod providers;
mod tui;

use app::Conversation;
use providers::Stub;
use tui::Chat;

fn run(terminal: &mut ratatui::DefaultTerminal, chat: &mut Chat) -> io::Result<()> {
    loop {
        terminal.draw(|frame| tui::draw(frame, chat))?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press && tui::handle_key(chat, key) {
                    return Ok(());
                }
            }
        }
    }
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let conversation = Conversation::new(Box::new(Stub));
    let mut chat = Chat::new(Box::new(conversation));
    let result = run(&mut terminal, &mut chat);
    ratatui::restore();
    result
}
