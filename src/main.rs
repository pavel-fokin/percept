use std::sync::Arc;

use clap::Parser;
use crossterm::event::{Event as CtEvent, EventStream, KeyEventKind};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

mod app;
mod cli;
mod percept;
mod providers;
mod shared;
mod store;
mod tui;

use app::App;
use cli::{Cli, Command, EventsCommand};
use providers::Ollama;
use store::Jsonl;
use tui::{Chat, StreamEvent};

/// Where the event log lives: `percept.jsonl` in the working directory,
/// so the transcript follows wherever the app is launched from.
const LOG_PATH: &str = "percept.jsonl";

/// Where the local ollama server listens.
const OLLAMA_URL: &str = "http://localhost:11434";
/// The model ollama serves replies with.
const OLLAMA_MODEL: &str = "gemma4";

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    chat: &mut Chat<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut term_events = EventStream::new();
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<StreamEvent>();

    loop {
        terminal.draw(|frame| tui::draw(frame, chat))?;

        tokio::select! {
            // Biased, so a finished reply is always committed before a
            // keypress that could quit. Unbiased, select! picks at
            // random and Esc can beat a queued Done, leaving the log
            // with a prompt and no answer.
            biased;

            Some(event) = reply_rx.recv() => {
                match event {
                    StreamEvent::Chunk(chunk) => chat.app.append_chunk(chunk),
                    StreamEvent::Done => chat.app.end_stream()?,
                    StreamEvent::Failed(message) => {
                        // Whatever the model managed to say is real and
                        // commits; the failure is only shown.
                        chat.app.end_stream()?;
                        chat.error = Some(message);
                    }
                }
            }
            Some(Ok(event)) = term_events.next() => {
                if let CtEvent::Key(key) = event {
                    if key.kind == KeyEventKind::Press
                        && tui::handle_key(chat, key, &reply_tx)?
                    {
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Arc::new(Jsonl::open(LOG_PATH)?);
    let model = Ollama::new(OLLAMA_URL.to_string(), OLLAMA_MODEL.to_string());
    let app = App::new(Arc::new(model), log, "tui".to_string())?;

    let mut terminal = ratatui::init();
    let mut chat = Chat::new(Box::new(app));
    let result = run(&mut terminal, &mut chat).await;
    ratatui::restore();
    result
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Command::Events { command }) => Jsonl::open(LOG_PATH)
            .map_err(Box::<dyn std::error::Error>::from)
            .and_then(|log| match command {
                EventsCommand::Publish(args) => cli::publish(args, &log),
                EventsCommand::Search(args) => cli::search(args, &log),
                EventsCommand::Show(args) => cli::show(args, &log),
            }),
        None => try_main().await,
    };

    if let Err(err) = result {
        eprintln!("percept: {err}");
        std::process::exit(1);
    }
}
