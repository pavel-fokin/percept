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
mod core;
mod harness;
mod mapstore;
mod providers;
mod shared;
mod store;
#[cfg(test)]
mod tests;
mod tools;
mod tui;

use crate::core::Actor;
use app::{App, Harness, MapShape};
use cli::{Cli, Command, EventsCommand, MapsCommand};
use mapstore::{LogMaps, MarkdownFiles};
use providers::{Catalog, ProviderConfig, FIREWORKS_MODEL, OPENAI_MODEL};
use store::Jsonl;
use tools::{
    AskBeforeWrites, Bash, EditFile, FindFiles, GitSnapshot, GrepFiles, ListFiles, ReadEvent,
    ReadFile, ReadMap, ReviseMap, SearchEvents, Workspace, WriteFile,
};
use tui::{Chat, StreamEvent};

/// Names the directory percept keeps its state in - the event log, and
/// the installed binary. Defaults to `~/.percept`.
const HOME_VAR: &str = "PERCEPT_HOME";

/// The event log's file name under `PERCEPT_HOME`. One log for every
/// project: an event's `source.path` says which one it came from.
const LOG_FILE: &str = "percept.jsonl";

/// Where `percept hook` keeps one file per turn - beside the log, so a
/// hook running from any checkout finds the same state a moment later
/// reads back.
const HOOK_SESSIONS_DIR: &str = "hook-sessions";

/// Names the provider that answers: `ollama` (the default), `openai`,
/// or `fireworks`.
const PROVIDER_VAR: &str = "PERCEPT_PROVIDER";

/// Source name percept's own coding agent stamps on every event it
/// commits - `claude-code` and `codex` are the other coding agents,
/// each reaching the log a different way.
const CODE_SOURCE_NAME: &str = "percept-code";

/// Source name the headless `ask`/`reflect` turns and the `maps` write
/// verbs stamp.
const CLI_SOURCE_NAME: &str = "percept-cli";

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

const FIREWORKS_URL: &str = "https://api.fireworks.ai/inference/v1";
/// Where the key is read from.
const FIREWORKS_KEY_VAR: &str = "FIREWORKS_API_KEY";

/// What `percept reflect` asks the model to do. One place to change it,
/// like the ollama settings above.
const REFLECT_PROMPT: &str = "Revise the decisions map from recent events. \
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

/// `$PERCEPT_HOME` when set. Otherwise, a binary running from
/// `target/debug` or `target/release` - a checkout being built and run
/// with `cargo`, not `scripts/install.sh`'s output - keeps its state
/// under the checkout's own `.percept/` instead, so iterating on
/// percept doesn't mix test events into the shared log. Any other
/// binary, installed or not, falls back to `~/.percept`. `HOME` unset
/// is an error there: there is nowhere to put the state, and a relative
/// default would scatter it per directory. `checkout` is the one `main`
/// resolved - from the client's cwd under `percept hook`, so a dev
/// build's hook keeps its state where the client is, not where the
/// process was started.
fn data_dir(checkout: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(home) = std::env::var_os(HOME_VAR).filter(|home| !home.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    if std::env::current_exe().is_ok_and(|exe| is_dev_build(&exe)) {
        return Ok(checkout.join(MAPS_DIR));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".percept"))
        .ok_or_else(|| format!("neither {HOME_VAR} nor HOME is set").into())
}

/// `data_dir(checkout)`'s event log file.
fn log_path(checkout: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(data_dir(checkout)?.join(LOG_FILE))
}

/// Whether `exe` sits under a `target/debug` or `target/release` -
/// `cargo build`'s output directories - rather than wherever
/// `scripts/install.sh` or a package manager put the binary.
fn is_dev_build(exe: &Path) -> bool {
    exe.ancestors().any(|dir| {
        matches!(
            dir.file_name().and_then(|name| name.to_str()),
            Some("debug" | "release")
        ) && dir.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("target"))
    })
}

/// The shared log, opened where `log_path` says.
fn open_log(checkout: &Path) -> Result<Jsonl, Box<dyn std::error::Error>> {
    Ok(Jsonl::open(log_path(checkout)?)?)
}

/// `percept hook <client>` - runs before the shared prelude below, since
/// it needs the client's own `cwd` to find the checkout, not this
/// process's. Never lets an error reach the client's turn: prints it to
/// stderr as `percept hook: <error>` and exits 1, `{}` on stdout either
/// way.
fn hook_main(args: cli::hook::HookArgs) -> ! {
    match hook_run(args) {
        Ok(output) => {
            println!("{output}");
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("percept hook: {err}");
            println!("{{}}");
            std::process::exit(1);
        }
    }
}

/// Reads and validates stdin, resolves the checkout and project root
/// from the client's own `cwd`, opens the log there, and dispatches to
/// `cli::hook::run` under `args.client`'s source.
fn hook_run(args: cli::hook::HookArgs) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut stdin = std::io::stdin().lock();
    let input = cli::hook::read(&mut stdin)?;
    let checkout = root_for(Path::new(input.cwd()))?;
    let root = project_of(&checkout);
    let source = crate::core::Source {
        name: args.client,
        path: root,
    };
    let log = open_log(&checkout)?;
    let sessions = data_dir(&checkout)?.join(HOOK_SESSIONS_DIR);
    cli::hook::run(input, &source, &log, &sessions)
}

/// The checkout `cwd` is in: the first ancestor of it
/// holding a `.git` or `.percept` entry - a repository, or a directory
/// percept has already rendered maps into. The search stops at `$HOME`
/// and at the filesystem root without matching either: walking a home
/// directory scans every project under it, and on macOS the
/// TCC-protected Desktop, Photos and Music, so a project sitting at
/// `$HOME` itself is not supported - work from a subdirectory. Errors
/// when nothing is found; the caller prints it and exits. cwd and
/// `$HOME` are both canonicalized first, so a symlinked home directory
/// still stops the walk and two writers started from a symlinked path
/// get the same root. `main` passes the process's own directory, or
/// the client's under `percept hook`, since the process's own may be
/// anywhere the client's shell happened to start it from.
fn root_for(cwd: &Path) -> std::io::Result<PathBuf> {
    let cwd = cwd.canonicalize()?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|home| home.canonicalize().ok());
    discover_root(&cwd, home.as_deref()).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "{} is not inside a project: no .git or .percept directory here or above. \
                 Run `git init` or `mkdir .percept` to make this one.",
                cwd.display()
            ),
        )
    })
}

/// The walk `root_for` runs, split out so it takes cwd and `$HOME`
/// as arguments; both are already canonical, so an ancestor reached
/// through `parent()` is too. `None` when the search reaches `home` or
/// the filesystem root before a marker.
fn discover_root(cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let mut dir = cwd;
    loop {
        if home == Some(dir) {
            return None;
        }
        if dir.join(".git").exists() || dir.join(".percept").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
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
/// checkout a worktree's `Source` names, since the files are here.
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
    ])
}

/// Routes `read_map` by name: the `code` map is walked fresh from the
/// working tree, every other map is folded from the log. Wiring only -
/// it is what keeps `store` from depending on `code`.
struct RoutedMaps {
    folded: LogMaps,
    root: PathBuf,
}

impl crate::core::MapReader for RoutedMaps {
    fn read(&self, name: &str) -> Result<crate::core::Map, Box<dyn std::error::Error>> {
        if name == crate::core::CODE {
            Ok(code::build(&self.root)?)
        } else {
            self.folded.read(name)
        }
    }
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
    renderer: Arc<dyn crate::core::MapRenderer>,
    checkout: &Path,
) -> Result<App, Box<dyn std::error::Error>> {
    let log = Arc::new(open_log(checkout)?);
    let schemas = Arc::new(mapstore::load_schemas(checkout)?);
    let catalog: Arc<dyn crate::harness::ModelCatalog> = Arc::new(build_catalog());
    let model = build_model(&*catalog)?;
    let map_shape = build_maps_shape()?;
    let scope = source.scope();
    let maps = RoutedMaps {
        folded: LogMaps::new(log.clone(), schemas.clone(), scope.clone()),
        root: checkout.to_path_buf(),
    };
    let mut tools: Vec<Arc<dyn crate::harness::Tool>> = vec![
        Arc::new(SearchEvents::new(log.clone())),
        Arc::new(ReadEvent::new(log.clone())),
        Arc::new(ReviseMap::new(log.clone(), schemas.clone(), scope)),
        Arc::new(ReadMap::new(Arc::new(maps))),
    ];
    match build_toolset(&source, checkout)? {
        Toolset::Maps => App::new(
            model,
            catalog,
            log,
            schemas,
            Harness::new(tools, map_shape),
            renderer,
            source,
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
            App::new(model, catalog, log, schemas, harness, renderer, source)
        }
    }
}

/// One turn without the TUI: `ask` with the user's prompt, `reflect`
/// with percept's own. `yes` is `ask --yes`.
async fn headless_turn(
    actor: Actor,
    prompt: String,
    yes: bool,
    source: crate::core::Source,
    renderer: Arc<dyn crate::core::MapRenderer>,
    checkout: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app(source, renderer, checkout)?;
    cli::run_turn(Box::new(app), actor, prompt, yes).await
}

async fn try_main(
    source: crate::core::Source,
    renderer: Arc<dyn crate::core::MapRenderer>,
    checkout: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app(source, renderer, checkout)?;

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

    // `percept hook` takes its cwd from the client's own JSON, not this
    // process's, since the client's shell may have started it anywhere;
    // it resolves its own checkout and never reaches the prelude below.
    if let Some(Command::Hook(args)) = cli.command {
        hook_main(args)
    }

    // `root` is the project a `Source` names and a `Scope` compares;
    // `checkout` is where the files are. They differ only in a linked
    // worktree, where the map is shared but its render, and the code
    // map, belong to the checkout being worked in.
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(err) => {
            eprintln!("percept: {err}");
            std::process::exit(1);
        }
    };
    let checkout = match root_for(&cwd) {
        Ok(checkout) => checkout,
        Err(err) => {
            eprintln!("percept: {err}");
            std::process::exit(1);
        }
    };
    let root = project_of(&checkout);
    let cli_source = crate::core::Source {
        name: CLI_SOURCE_NAME.to_string(),
        path: root.clone(),
    };
    let renderer: Arc<dyn crate::core::MapRenderer> =
        Arc::new(MarkdownFiles::new(checkout.join(MAPS_DIR)));

    let result = match cli.command {
        // `hook_main` above exits before this match is ever reached.
        Some(Command::Hook(_)) => unreachable!(),
        Some(Command::Events { command }) => open_log(&checkout).and_then(|log| match command {
            EventsCommand::Publish(args) => cli::publish(args, &log, &root),
            EventsCommand::Search(args) => cli::search(args, &log),
            EventsCommand::Show(args) => cli::show(args, &log),
        }),
        // `maps show code` is walked fresh from the working tree, never
        // the log, so it must not even open the log.
        Some(Command::Maps {
            command: MapsCommand::Show(args),
        }) if args.is_code() => cli::maps_show_code(args, &checkout),
        Some(Command::Maps { command }) => open_log(&checkout).and_then(|log| {
            let schemas = mapstore::load_schemas(&checkout)?;
            match command {
                MapsCommand::List(args) => cli::maps_list(args, &log, &schemas, &root, &checkout),
                MapsCommand::Show(args) => cli::maps_show(args, &log, &schemas, &root),
                MapsCommand::AddNode(args) => {
                    cli::maps_add_node(args, &log, &schemas, &cli_source, renderer.as_ref())
                }
                MapsCommand::AddEdge(args) => {
                    cli::maps_add_edge(args, &log, &schemas, &cli_source, renderer.as_ref())
                }
                MapsCommand::RemoveNode(args) => {
                    cli::maps_remove_node(args, &log, &schemas, &cli_source, renderer.as_ref())
                }
                MapsCommand::RemoveEdge(args) => {
                    cli::maps_remove_edge(args, &log, &schemas, &cli_source, renderer.as_ref())
                }
            }
        }),
        Some(Command::Ask(args)) => {
            headless_turn(
                Actor::User,
                args.prompt,
                args.yes,
                cli_source,
                renderer,
                &checkout,
            )
            .await
        }
        Some(Command::Init(args)) => cli::init::run(args, &checkout),
        Some(Command::Reflect) => {
            headless_turn(
                Actor::System,
                REFLECT_PROMPT.to_string(),
                false,
                cli_source,
                renderer,
                &checkout,
            )
            .await
        }
        None => {
            try_main(
                crate::core::Source {
                    name: CODE_SOURCE_NAME.to_string(),
                    path: root,
                },
                renderer,
                &checkout,
            )
            .await
        }
    };

    if let Err(err) = result {
        eprintln!("percept: {err}");
        std::process::exit(1);
    }
    // An explicit exit, not a fall off the end: dropping the runtime
    // waits for every blocking task, and a `bash` call the user quit
    // in the middle of can hold one for minutes.
    std::process::exit(0);
}
