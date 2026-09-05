use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as CtEvent, EventStream, KeyEventKind,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

mod app;
mod cli;
mod code;
mod percept;
mod providers;
mod shared;
mod store;
#[cfg(test)]
mod testing;
mod tui;

use app::{App, MapShape};
use cli::{Cli, Command, EventsCommand, MapsCommand};
use percept::Actor;
use providers::{Catalog, OPENAI_MODEL};
use store::{Jsonl, ReadEvent, ReadMap, ReviseMap, SearchEvents};
use tui::{Chat, StreamEvent};

/// Names the directory percept keeps its state in - the event log, and
/// the installed binary. Defaults to `~/.percept`.
const HOME_VAR: &str = "PERCEPT_HOME";

/// The event log's file name under `PERCEPT_HOME`. One log for every
/// project: an event's `source.path` says which one it came from.
const LOG_FILE: &str = "percept.jsonl";

/// Names the provider that answers: `ollama` (the default) or `openai`.
const PROVIDER_VAR: &str = "PERCEPT_PROVIDER";

/// Source name the TUI stamps on every event it commits.
const TUI_SOURCE_NAME: &str = "percept-tui";

/// Source name the headless `ask`/`reflect` turns and the `maps` write
/// verbs stamp.
const CLI_SOURCE_NAME: &str = "percept-cli";

/// Names how much of each cognitive map reaches the model each turn:
/// `prompt` (the default, today's behaviour), `headlines`, or `tool`.
const MAPS_VAR: &str = "PERCEPT_MAPS";

/// Where a project's maps are rendered as Markdown, under its root -
/// rerendered on every write, so a reader who never runs `percept`
/// still sees the latest fold.
const MAPS_DIR: &str = ".percept";

/// Where the local ollama server listens.
const OLLAMA_URL: &str = "http://localhost:11434";
/// The model ollama serves replies with.
const OLLAMA_MODEL: &str = "gemma4";

const OPENAI_URL: &str = "https://api.openai.com/v1";
/// How long the model thinks before answering. Low keeps a turn quick
/// while still letting it plan a search.
const OPENAI_REASONING: &str = "low";
/// Where the key is read from.
const OPENAI_KEY_VAR: &str = "OPENAI_API_KEY";

/// What `percept reflect` asks the model to do. One place to change it,
/// like the ollama settings above.
const REFLECT_PROMPT: &str = "Revise the decisions map from recent events. \
    First call search_events for questions raised, options weighed, \
    evidence given, and decisions taken that the map does not yet hold; \
    only its results carry event ids. Then record them with revise_map, \
    citing those ids in each node's sources - a node without one is \
    refused. Remove what no longer holds, with a reason. Reply with a \
    short summary of what changed, or say the map already held \
    everything.";

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

/// `$PERCEPT_HOME/percept.jsonl`, or `~/.percept/percept.jsonl` when the
/// variable is unset or empty. `HOME` unset is an error: there is
/// nowhere to put the log, and a relative default would scatter logs
/// per directory.
fn log_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = match std::env::var_os(HOME_VAR).filter(|home| !home.is_empty()) {
        Some(home) => PathBuf::from(home),
        None => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".percept"))
            .ok_or(format!("neither {HOME_VAR} nor HOME is set"))?,
    };
    Ok(home.join(LOG_FILE))
}

/// The shared log, opened where `log_path` says.
fn open_log() -> Result<Jsonl, Box<dyn std::error::Error>> {
    Ok(Jsonl::open(log_path()?)?)
}

/// The checkout the current directory is in: walks up looking for a
/// `.git` entry - the closest thing to a repository root without
/// shelling out to git. Falls back to the current directory when none
/// is found. Canonicalized either way, so a path always comes out
/// absolute, and two writers started from a symlinked path get the
/// same one.
fn checkout_root() -> std::io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();
    loop {
        if dir.join(".git").exists() {
            return dir.canonicalize();
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return cwd.canonicalize(),
        }
    }
}

/// The project a checkout belongs to - what a `Source` names and a
/// `Scope` compares. A linked worktree's `.git` is a file pointing at
/// the main checkout's `.git/worktrees/<name>`; the project is that
/// main checkout, so every worktree of one repository shares one map
/// instead of each starting empty. A second clone at another path is
/// another project: the path is the identity, and nothing here reads
/// remotes to say otherwise.
fn project_of(checkout: &Path) -> PathBuf {
    let Ok(text) = std::fs::read_to_string(checkout.join(".git")) else {
        return checkout.to_path_buf();
    };
    text.strip_prefix("gitdir:")
        .map(|gitdir| Path::new(gitdir.trim()))
        .and_then(|gitdir| {
            gitdir
                .ancestors()
                .find(|dir| dir.file_name().is_some_and(|name| name == ".git"))
        })
        .and_then(Path::parent)
        .and_then(|main| main.canonicalize().ok())
        .unwrap_or_else(|| checkout.to_path_buf())
}

/// Builds the model `PERCEPT_PROVIDER` names through `catalog`, so the
/// provider dispatch lives once. Still checks `OPENAI_KEY_VAR` here,
/// eagerly, unlike `catalog` itself: a run that starts with
/// `PERCEPT_PROVIDER=openai` and no key set should fail at startup, not
/// on the first reply - but `/models` switching shouldn't need the key
/// set just to list models, so `catalog` stays lenient about it.
fn build_model(
    catalog: &dyn percept::ModelCatalog,
) -> Result<Arc<dyn percept::Model>, Box<dyn std::error::Error>> {
    let provider = std::env::var(PROVIDER_VAR).unwrap_or_else(|_| "ollama".to_string());
    let (provider, model) = match provider.as_str() {
        "ollama" => (percept::Provider::Ollama, OLLAMA_MODEL.to_string()),
        "openai" => {
            std::env::var(OPENAI_KEY_VAR).map_err(|_| format!("{OPENAI_KEY_VAR} is not set"))?;
            (percept::Provider::OpenAi, OPENAI_MODEL.to_string())
        }
        other => {
            return Err(
                format!("{PROVIDER_VAR}={other:?} names no provider; use ollama or openai").into(),
            )
        }
    };
    catalog.build(&percept::ModelDescriptor { provider, model })
}

/// `OPENAI_KEY_VAR` is read leniently here, unlike `build_model`'s
/// openai branch, since a run that never asks for an openai model
/// shouldn't need the key set.
fn build_catalog() -> Catalog {
    let api_key = std::env::var(OPENAI_KEY_VAR).unwrap_or_default();
    Catalog::new(
        OLLAMA_URL.to_string(),
        OPENAI_URL.to_string(),
        api_key,
        OPENAI_REASONING.to_string(),
    )
}

fn build_maps_shape() -> Result<MapShape, Box<dyn std::error::Error>> {
    let shape = std::env::var(MAPS_VAR).unwrap_or_else(|_| "prompt".to_string());
    match shape.as_str() {
        "prompt" => Ok(MapShape::Prompt),
        "headlines" => Ok(MapShape::Headlines),
        "tool" => Ok(MapShape::Tool),
        other => Err(
            format!("{MAPS_VAR}={other:?} names no shape; use prompt, headlines or tool").into(),
        ),
    }
}

/// Both the TUI and `ask` build the same `App` this way, differing only
/// in the `Source` they stamp and in how they drive its reply stream.
fn build_app(
    source: percept::Source,
    renderer: Arc<dyn percept::MapRenderer>,
) -> Result<App, Box<dyn std::error::Error>> {
    let log = Arc::new(open_log()?);
    let catalog: Arc<dyn percept::ModelCatalog> = Arc::new(build_catalog());
    let model = build_model(&*catalog)?;
    let map_shape = build_maps_shape()?;
    let scope = source.scope();
    let mut tools: Vec<Arc<dyn percept::Tool>> = vec![
        Arc::new(SearchEvents::new(log.clone())),
        Arc::new(ReadEvent::new(log.clone())),
        Arc::new(ReviseMap::new(log.clone(), scope.clone())),
    ];
    if map_shape.opens_by_tool() {
        tools.push(Arc::new(ReadMap::new(log.clone(), scope)));
    }
    App::new(model, catalog, log, tools, renderer, map_shape, source)
}

/// One turn without the TUI: `ask` with the user's prompt, `reflect`
/// with percept's own.
async fn headless_turn(
    actor: Actor,
    prompt: String,
    source: percept::Source,
    renderer: Arc<dyn percept::MapRenderer>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app(source, renderer)?;
    cli::run_turn(Box::new(app), actor, prompt).await
}

async fn try_main(
    source: percept::Source,
    renderer: Arc<dyn percept::MapRenderer>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app(source, renderer)?;

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

    // `root` is the project a `Source` names and a `Scope` compares;
    // `checkout` is where the files are. They differ only in a linked
    // worktree, where the map is shared but its render, and the code
    // map, belong to the checkout being worked in.
    let checkout = match checkout_root() {
        Ok(checkout) => checkout,
        Err(err) => {
            eprintln!("percept: {err}");
            std::process::exit(1);
        }
    };
    let root = project_of(&checkout);
    let cli_source = percept::Source {
        name: CLI_SOURCE_NAME.to_string(),
        path: root.clone(),
    };
    let renderer: Arc<dyn percept::MapRenderer> =
        Arc::new(store::MarkdownFiles::new(checkout.join(MAPS_DIR)));

    let result = match cli.command {
        Some(Command::Events { command }) => open_log().and_then(|log| match command {
            EventsCommand::Publish(args) => cli::publish(args, &log, &root),
            EventsCommand::Search(args) => cli::search(args, &log),
            EventsCommand::Show(args) => cli::show(args, &log),
        }),
        // `maps show code` is walked fresh from the working tree, never
        // the log, so it must not even open the log.
        Some(Command::Maps {
            command: MapsCommand::Show(args),
        }) if args.is_code() => cli::maps_show_code(args, &checkout),
        Some(Command::Maps { command }) => open_log().and_then(|log| match command {
            MapsCommand::List(args) => cli::maps_list(args, &log, &root, &checkout),
            MapsCommand::Show(args) => cli::maps_show(args, &log, &root),
            MapsCommand::AddNode(args) => {
                cli::maps_add_node(args, &log, &cli_source, renderer.as_ref())
            }
            MapsCommand::AddEdge(args) => {
                cli::maps_add_edge(args, &log, &cli_source, renderer.as_ref())
            }
            MapsCommand::RemoveNode(args) => {
                cli::maps_remove_node(args, &log, &cli_source, renderer.as_ref())
            }
            MapsCommand::RemoveEdge(args) => {
                cli::maps_remove_edge(args, &log, &cli_source, renderer.as_ref())
            }
        }),
        Some(Command::Ask(args)) => {
            headless_turn(Actor::User, args.prompt, cli_source, renderer).await
        }
        Some(Command::Reflect) => {
            headless_turn(
                Actor::System,
                REFLECT_PROMPT.to_string(),
                cli_source,
                renderer,
            )
            .await
        }
        None => {
            try_main(
                percept::Source {
                    name: TUI_SOURCE_NAME.to_string(),
                    path: root,
                },
                renderer,
            )
            .await
        }
    };

    if let Err(err) = result {
        eprintln!("percept: {err}");
        std::process::exit(1);
    }
}
