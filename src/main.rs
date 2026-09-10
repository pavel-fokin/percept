use std::path::{Path, PathBuf};

use clap::Parser;

#[cfg(feature = "lab")]
mod app;
mod cli;
#[cfg(feature = "lab")]
mod code;
mod core;
#[cfg(feature = "lab")]
mod harness;
#[cfg(feature = "lab")]
mod lab;
mod mapstore;
#[cfg(feature = "lab")]
mod providers;
mod server;
mod shared;
mod store;
#[cfg(test)]
mod tests;
#[cfg(feature = "lab")]
mod tools;
#[cfg(feature = "lab")]
mod tui;
mod workspace;

use cli::{Cli, Command, EventsCommand, MapsCommand};
use store::Jsonl;

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

/// Source name the headless `ask`/`reflect` turns and the `maps` write
/// verbs stamp.
const CLI_SOURCE_NAME: &str = "percept-cli";

/// Where a project's maps are rendered as Markdown, under its root -
/// rerendered on every write, so a reader who never runs `percept`
/// still sees the latest fold.
const MAPS_DIR: &str = ".percept";

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
    let me = log.me();
    let sessions = data_dir(&checkout)?.join(HOOK_SESSIONS_DIR);
    cli::hook::run(input, &source, &log, &sessions, &checkout, me)
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
    let result = match cli.command {
        // `hook_main` above exits before this match is ever reached.
        Some(Command::Hook(_)) => unreachable!(),
        Some(Command::Events { command }) => open_log(&checkout).and_then(|log| {
            let me = log.me();
            match command {
                EventsCommand::Publish(args) => cli::publish(args, &log, &root, &checkout, me),
                EventsCommand::Search(args) => cli::search(args, &log, me),
                EventsCommand::Show(args) => cli::show(args, &log),
            }
        }),
        Some(Command::Maps { command }) => open_log(&checkout).and_then(|log| {
            let schemas = mapstore::load_schemas(&checkout)?;
            let me = log.me();
            match command {
                MapsCommand::List(args) => cli::maps_list(args, &log, &schemas, &root),
                MapsCommand::Show(args) => cli::maps_show(args, &log, &schemas, &root),
                MapsCommand::AddNode(args) => {
                    cli::maps_add_node(args, &log, &schemas, &cli_source, me)
                }
                MapsCommand::AddEdge(args) => {
                    cli::maps_add_edge(args, &log, &schemas, &cli_source, me)
                }
                MapsCommand::RemoveNode(args) => {
                    cli::maps_remove_node(args, &log, &schemas, &cli_source, me)
                }
                MapsCommand::RemoveEdge(args) => {
                    cli::maps_remove_edge(args, &log, &schemas, &cli_source, me)
                }
                MapsCommand::Record(args) => {
                    cli::maps_record(args, &log, &schemas, &cli_source, &checkout, me)
                }
                MapsCommand::Confirm(args) => {
                    cli::maps_confirm(args, &log, &schemas, &cli_source, me)
                }
                MapsCommand::Dispute(args) => {
                    cli::maps_dispute(args, &log, &schemas, &cli_source, me)
                }
            }
        }),
        #[cfg(feature = "lab")]
        Some(Command::Ask(args)) => {
            lab::headless_turn(false, args.prompt, args.yes, cli_source, &checkout).await
        }
        Some(Command::Init(args)) => cli::init::run(args, &checkout),
        Some(Command::Review) => {
            let opened = open_log(&checkout).and_then(|log| {
                let schemas = mapstore::load_schemas(&checkout)?;
                let me = log.me();
                Ok((log, schemas, me))
            });
            match opened {
                Ok((log, schemas, me)) => {
                    server::run(std::sync::Arc::new(log), schemas, cli_source.clone(), me).await
                }
                Err(err) => Err(err),
            }
        }
        #[cfg(feature = "lab")]
        Some(Command::Reflect) => {
            lab::headless_turn(
                true,
                lab::REFLECT_PROMPT.to_string(),
                false,
                cli_source,
                &checkout,
            )
            .await
        }
        #[cfg(feature = "lab")]
        None => {
            lab::try_main(
                crate::core::Source {
                    name: lab::CODE_SOURCE_NAME.to_string(),
                    path: root,
                },
                &checkout,
            )
            .await
        }
        // Without the lab, clap's `arg_required_else_help` on `Cli`
        // has already printed help and exited.
        #[cfg(not(feature = "lab"))]
        None => unreachable!(),
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
