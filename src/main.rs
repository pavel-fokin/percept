use std::sync::Arc;

use clap::Parser;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as CtEvent, EventStream, KeyEventKind,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

mod app;
mod cli;
mod percept;
mod providers;
mod shared;
mod store;
#[cfg(test)]
mod testing;
mod tui;

use app::App;
use cli::{Cli, Command, EventsCommand};
use providers::Ollama;
use store::{Jsonl, SearchEvents};
use tui::{Chat, StreamEvent};

/// Where the event log lives: `percept.jsonl` in the working directory,
/// so the transcript follows wherever the app is launched from.
const LOG_PATH: &str = "percept.jsonl";

/// Where the local ollama server listens.
const OLLAMA_URL: &str = "http://localhost:11434";
/// The model ollama serves replies with.
const OLLAMA_MODEL: &str = "gemma4";

/// How often the status row's spinner advances while a turn streams.
const SPINNER_TICK: std::time::Duration = std::time::Duration::from_millis(90);

/// Mouse capture is outside ratatui's terminal setup, so it needs its
/// own guard for both normal exits and unwinding.
struct MouseCapture;

impl MouseCapture {
    fn enable() -> std::io::Result<Self> {
        crossterm::execute!(std::io::stdout(), EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for MouseCapture {
    fn drop(&mut self) {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    }
}

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
                tui::handle_stream(chat, event, &reply_tx)?;
                // Tokens arrive far faster than a frame is worth
                // drawing, and every frame re-wraps the whole
                // transcript. Applying the queue first costs one frame
                // per burst instead of one per token.
                while let Ok(event) = reply_rx.try_recv() {
                    tui::handle_stream(chat, event, &reply_tx)?;
                }
            }
            // Only while a turn streams: idle, nothing moves, so
            // there is no frame worth redrawing.
            _ = tokio::time::sleep(SPINNER_TICK), if chat.app.is_replying() => {
                chat.tick();
            }
            Some(Ok(event)) = term_events.next() => {
                if let CtEvent::Key(key) = event {
                    if key.kind == KeyEventKind::Press
                        && tui::handle_key(chat, key, &reply_tx)?
                    {
                        return Ok(());
                    }
                } else if let CtEvent::Mouse(mouse) = event {
                    tui::handle_mouse(chat, mouse);
                }
            }
        }
    }
}

/// Both the TUI and `ask` build the same `App` this way, differing only
/// in the `source` they stamp and in how they drive its reply stream.
fn build_app(source: &str) -> Result<App, Box<dyn std::error::Error>> {
    let log = Arc::new(Jsonl::open(LOG_PATH)?);
    let model = Ollama::new(OLLAMA_URL.to_string(), OLLAMA_MODEL.to_string());
    let tools: Vec<Arc<dyn percept::Tool>> = vec![Arc::new(SearchEvents::new(log.clone()))];
    App::new(Arc::new(model), log, tools, source.to_string())
}

async fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app("tui")?;

    let mut terminal = ratatui::init();
    let mouse = match MouseCapture::enable() {
        Ok(mouse) => mouse,
        Err(err) => {
            ratatui::restore();
            return Err(err.into());
        }
    };
    let mut chat = Chat::new(Box::new(app));
    let result = run(&mut terminal, &mut chat).await;
    drop(mouse);
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
        Some(Command::Ask(args)) => match build_app("cli") {
            Ok(app) => cli::ask(args, Box::new(app)).await,
            Err(err) => Err(err),
        },
        None => try_main().await,
    };

    if let Err(err) = result {
        eprintln!("percept: {err}");
        std::process::exit(1);
    }
}
