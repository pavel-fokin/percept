use std::path::Path;
use std::sync::Arc;

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as CtEvent, EventStream, KeyEventKind,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::core::Actor;
use crate::app::{App, AppService, Harness, MapShape};
use crate::mapstore::LogMaps;
use crate::providers::{Catalog, ProviderConfig, FIREWORKS_MODEL, OPENAI_MODEL};
use crate::tools::{
    AskBeforeWrites, Bash, EditFile, FindFiles, GitSnapshot, GrepFiles, ListFiles, ReadCode,
    ReadEvent, ReadFile, ReadMap, ReviseMap, SearchEvents, WriteFile,
};
use crate::tui::{self, Chat, StreamEvent};
use crate::workspace::Workspace;

#[cfg(test)]
mod tests;

/// Names the provider that answers: `ollama` (the default), `openai`,
/// or `fireworks`.
const PROVIDER_VAR: &str = "PERCEPT_PROVIDER";

/// Source name percept's own coding agent stamps on every event it
/// commits - `claude-code` and `codex` are the other coding agents,
/// each reaching the log a different way.
pub const CODE_SOURCE_NAME: &str = "percept-code";

/// Names how much of each cognitive map reaches the model each turn:
/// `prompt` (the default, today's behaviour), `headlines`, or `tool`.
const MAPS_VAR: &str = "PERCEPT_MAPS";

/// Names which tools a turn carries. The TUI defaults to `code` in a git
/// checkout and `maps` without one; headless commands always default to
/// `maps`. Setting this variable overrides the default.
const TOOLS_VAR: &str = "PERCEPT_TOOLS";

/// Most tool calls a coding turn may make. A coding task reads several
/// files before one edit; the map tools' cap of five would end it
/// mid-read.
const CODE_TOOL_CAP: usize = 50;

/// The project's instructions to a coding agent, at the checkout
/// root: the client-neutral file this repo keeps its own in. Read once
/// at startup and sent every round while the coding tools are on; a
/// project without one gets none.
const INSTRUCTIONS_FILE: &str = "AGENTS.md";

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

const FIREWORKS_URL: &str = "https://api.fireworks.ai/inference/v1";
/// Where the key is read from.
const FIREWORKS_KEY_VAR: &str = "FIREWORKS_API_KEY";

/// What `percept reflect` asks the model to do. One place to change it,
/// like the ollama settings above.
pub const REFLECT_PROMPT: &str = "Revise the decisions map from recent events. \
    First call search_events for questions raised, options weighed, \
    evidence given, and decisions taken that the map does not yet hold; \
    only its results carry event ids. Then record them with revise_map, \
    citing those ids in each node's sources - a node without one is \
    refused. A decision that no longer holds is not removed: add the one \
    that replaces it with a supersedes edge to the old, so the old stays \
    one hop away. Reply with a short summary of what changed, or say the \
    map already held everything.";

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

/// Builds the model `PERCEPT_PROVIDER` names through `catalog`, so the
/// provider dispatch lives once. Still checks `OPENAI_KEY_VAR` here,
/// eagerly, unlike `catalog` itself: a run that starts with
/// `PERCEPT_PROVIDER=openai` and no key set should fail at startup, not
/// on the first reply - but `/models` switching shouldn't need the key
/// set just to list models, so `catalog` stays lenient about it.
fn build_model(
    catalog: &dyn crate::harness::ModelCatalog,
) -> Result<Arc<dyn crate::harness::Model>, Box<dyn std::error::Error>> {
    let provider = std::env::var(PROVIDER_VAR).unwrap_or_else(|_| "ollama".to_string());
    let (provider, model) = match provider.as_str() {
        "ollama" => (crate::harness::Provider::Ollama, OLLAMA_MODEL.to_string()),
        "openai" => {
            std::env::var(OPENAI_KEY_VAR).map_err(|_| format!("{OPENAI_KEY_VAR} is not set"))?;
            (crate::harness::Provider::OpenAi, OPENAI_MODEL.to_string())
        }
        "fireworks" => {
            std::env::var(FIREWORKS_KEY_VAR)
                .map_err(|_| format!("{FIREWORKS_KEY_VAR} is not set"))?;
            (
                crate::harness::Provider::Fireworks,
                FIREWORKS_MODEL.to_string(),
            )
        }
        other => {
            return Err(format!(
                "{PROVIDER_VAR}={other:?} names no provider; use ollama, openai or fireworks"
            )
            .into())
        }
    };
    catalog.build(&crate::harness::ModelDescriptor {
        provider,
        model,
        reasoning_efforts: &[],
    })
}

/// `OPENAI_KEY_VAR` is read leniently here, unlike `build_model`'s
/// openai branch, since a run that never asks for an openai model
/// shouldn't need the key set.
fn build_catalog() -> Catalog {
    let openai = ProviderConfig {
        url: OPENAI_URL.to_string(),
        api_key: std::env::var(OPENAI_KEY_VAR).unwrap_or_default(),
    };
    let fireworks = ProviderConfig {
        url: FIREWORKS_URL.to_string(),
        api_key: std::env::var(FIREWORKS_KEY_VAR).unwrap_or_default(),
    };
    Catalog::new(
        OLLAMA_URL.to_string(),
        openai,
        OPENAI_REASONING.to_string(),
        fireworks,
    )
}

/// Which tools a turn carries: `Maps` for the log and map tools alone,
/// `Code` to add the file tools and `bash`. `resolve_toolset` picks one
/// from the client, `TOOLS_VAR`, and whether a git snapshot is possible.
enum Toolset {
    Maps,
    Code,
}

/// The TUI defaults to `Code`, but only where `snapshot_ok` - the code
/// toolset's undo is a git commit, and a `.percept`-only project has no
/// repository to make one in. `TOOLS_VAR` overrides the default; an
/// explicit `code` there is honoured even without a repository, and then
/// fails later with git's own error.
fn resolve_toolset(
    source_name: &str,
    configured: Option<&str>,
    snapshot_ok: bool,
) -> Result<Toolset, Box<dyn std::error::Error>> {
    let default = if source_name == CODE_SOURCE_NAME && snapshot_ok {
        "code"
    } else {
        "maps"
    };
    match configured.unwrap_or(default) {
        "maps" => Ok(Toolset::Maps),
        "code" => Ok(Toolset::Code),
        other => Err(format!("{TOOLS_VAR}={other:?} names no toolset; use maps or code").into()),
    }
}

fn build_toolset(
    source: &crate::core::Source,
    checkout: &Path,
) -> Result<Toolset, Box<dyn std::error::Error>> {
    // A `.git` entry - directory in a clone, file in a linked worktree -
    // is what `GitSnapshot::open` needs; a `.percept`-only project has none.
    let snapshot_ok = checkout.join(".git").exists();
    resolve_toolset(
        &source.name,
        std::env::var(TOOLS_VAR).ok().as_deref(),
        snapshot_ok,
    )
}

/// The file tools, over the checkout being worked in - never the main
/// checkout a worktree's `Source` names, since the files are here -
/// plus `read_code`, which walks the same checkout fresh on every call.
fn code_tools(
    checkout: &Path,
) -> Result<Vec<Arc<dyn crate::harness::Tool>>, Box<dyn std::error::Error>> {
    let workspace = Arc::new(Workspace::new(checkout)?);
    Ok(vec![
        Arc::new(ReadFile::new(workspace.clone())),
        Arc::new(WriteFile::new(workspace.clone())),
        Arc::new(EditFile::new(workspace.clone())),
        Arc::new(ListFiles::new(workspace.clone())),
        Arc::new(FindFiles::new(workspace.clone())),
        Arc::new(GrepFiles::new(workspace.clone())),
        Arc::new(Bash::new(workspace)),
        Arc::new(ReadCode::new(checkout.to_path_buf())),
    ])
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
/// The `code` toolset - the TUI's default, or `PERCEPT_TOOLS=code`
/// elsewhere - adds the file tools over `checkout`, with the policy, cap
/// and snapshot a turn that changes files needs.
fn build_app(
    source: crate::core::Source,
    checkout: &Path,
) -> Result<App, Box<dyn std::error::Error>> {
    let opened = super::open_log(checkout)?;
    let me = opened.me();
    let log = Arc::new(opened);
    let schemas = Arc::new(crate::mapstore::load_schemas(checkout)?);
    let catalog: Arc<dyn crate::harness::ModelCatalog> = Arc::new(build_catalog());
    let model = build_model(&*catalog)?;
    let map_shape = build_maps_shape()?;
    let path = source.path.clone();
    let maps = LogMaps::new(log.clone(), schemas.clone(), path.clone());
    let mut tools: Vec<Arc<dyn crate::harness::Tool>> = vec![
        Arc::new(SearchEvents::new(log.clone(), me)),
        Arc::new(ReadEvent::new(log.clone())),
        Arc::new(ReviseMap::new(log.clone(), schemas.clone(), path)),
        Arc::new(ReadMap::new(Arc::new(maps))),
    ];
    match build_toolset(&source, checkout)? {
        Toolset::Maps => App::new(
            model,
            catalog,
            log,
            schemas,
            Harness::new(tools, map_shape),
            source,
            me,
        ),
        Toolset::Code => {
            tools.extend(code_tools(checkout)?);
            let instructions = std::fs::read_to_string(checkout.join(INSTRUCTIONS_FILE)).ok();
            let harness = Harness {
                policy: Arc::new(AskBeforeWrites),
                tool_cap: CODE_TOOL_CAP,
                snapshot: Some(Arc::new(GitSnapshot::open(checkout)?)),
                instructions,
                ..Harness::new(tools, map_shape)
            };
            App::new(model, catalog, log, schemas, harness, source, me)
        }
    }
}

/// One turn without the TUI: `ask` with the user's prompt, attributed
/// to `app.me()`, or `reflect` with percept's own, as `Actor::System`.
/// `yes` is `ask --yes`.
pub async fn headless_turn(
    system: bool,
    prompt: String,
    yes: bool,
    source: crate::core::Source,
    checkout: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app(source, checkout)?;
    let actor = if system {
        Actor::System
    } else {
        Actor::Human(app.me())
    };
    crate::cli::run_turn(Box::new(app), actor, prompt, yes).await
}

pub async fn try_main(
    source: crate::core::Source,
    checkout: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app(source, checkout)?;

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
