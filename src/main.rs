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

use cli::{Cli, Command};
use store::{turn_dir, Jsonl, TurnState};

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
    let home = home_dir();
    let checkout = root_for(Path::new(input.cwd()), home.as_deref())?;
    let root = project_of(&checkout);
    let source = crate::core::Source {
        name: args.client,
        path: root,
    };
    let log = open_log(&checkout)?;
    let me = log.me();
    cli::hook::run(
        input,
        &source,
        &log,
        &sessions_dir(&checkout)?,
        &checkout,
        home.as_deref(),
        me,
    )
}

/// Where `percept hook` keeps every checkout's turns, beside the log.
fn sessions_dir(checkout: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(data_dir(checkout)?.join(HOOK_SESSIONS_DIR))
}

/// The id of the coding client's turn this map write runs in, when
/// there is one - what `percept hook`'s `UserPromptSubmit` pointed at
/// under `root`'s turn directory.
fn turn_cause(checkout: &Path, root: &Path) -> Result<Option<crate::core::EventId>, Box<dyn std::error::Error>> {
    Ok(TurnState::latest_cause(&turn_dir(&sessions_dir(checkout)?, root))?)
}

/// `$HOME`, canonicalized - `None` when it is unset or does not exist.
/// Where `root_for`'s walk stops and a global schema's map lives.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|home| home.canonicalize().ok())
}

/// The checkout `cwd` is in: the first ancestor of it
/// holding a `.git` or `.percept` entry - a repository, or a directory
/// percept has already rendered maps into. The search stops at `home`,
/// which is then the root itself: the outer level, where the global
/// maps live, never a project - a marker at `$HOME` is ignored, since
/// walking a home directory scans every project under it, and on macOS
/// the TCC-protected Desktop, Photos and Music. Errors when the walk
/// reaches the filesystem root with neither; the caller prints it and
/// exits. `cwd` is
/// canonicalized, as `home_dir` is, so a symlinked home directory
/// still stops the walk and two writers started from a symlinked path
/// get the same root. `main` passes the process's own directory, or
/// the client's under `percept hook`, since the process's own may be
/// anywhere the client's shell happened to start it from.
fn root_for(cwd: &Path, home: Option<&Path>) -> std::io::Result<PathBuf> {
    let cwd = cwd.canonicalize()?;
    discover_root(&cwd, home).ok_or_else(|| {
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
/// through `parent()` is too. `home` itself when the search reaches it
/// before a marker; `None` when it reaches the filesystem root.
fn discover_root(cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let mut dir = cwd;
    loop {
        if home == Some(dir) {
            return Some(dir.to_path_buf());
        }
        if dir.join(".git").exists() || dir.join(".percept").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// The project a checkout belongs to - what a `Source` names and
/// `mapstore::of_path` compares against. A linked worktree's `.git` is
/// a file pointing at
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

/// Whether `command` works on a checkout - its files or its config -
/// and so has nothing to work on at `$HOME`, where the root is the
/// home level itself. Exhaustive over `Command`, so a command added
/// later must say which it is.
fn needs_project(command: &Command) -> bool {
    match command {
        Command::Init(_) => true,
        #[cfg(feature = "lab")]
        Command::Ask(_) | Command::Code => true,
        Command::Search(_) | Command::Add(_) | Command::Remove(_) | Command::Change(_)
        | Command::Show(_) | Command::Web => false,
        // `hook_main` exits before `main`'s own match is ever reached.
        Command::Hook(_) => false,
    }
}

/// What `writer_context` builds: the schemas, the log opened under a
/// checkout, the human it resolves `--actor human` to, and the coding
/// client turn's own cause.
type WriterContext = (
    Box<dyn crate::core::Schemas>,
    Jsonl,
    Option<crate::core::HumanId>,
    Option<crate::core::EventId>,
);

/// The pieces `add`, `remove`, and `change` each build a `cli::Writer`
/// from: the schemas at `project` (`None` at the home level) and
/// `home`, the log opened under `checkout`, the human it resolves
/// `--actor human` to, and the coding client turn's own cause, when
/// `checkout` is in one.
fn writer_context(
    checkout: &Path,
    root: &Path,
    project: Option<&Path>,
    home: Option<&Path>,
) -> Result<WriterContext, Box<dyn std::error::Error>> {
    let schemas = mapstore::load_schemas(project, home)?;
    let log = open_log(checkout)?;
    let me = log.me();
    let cause = turn_cause(checkout, root)?;
    Ok((Box::new(schemas), log, me, cause))
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

    // `root` is the project a `Source` names and `mapstore::of_path`
    // compares against; `checkout` is where the files are. They differ
    // only in a linked worktree, where the map is shared but its
    // render, and the code map, belong to the checkout being worked in.
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(err) => {
            eprintln!("percept: {err}");
            std::process::exit(1);
        }
    };
    let home = home_dir();
    let checkout = match root_for(&cwd, home.as_deref()) {
        Ok(checkout) => checkout,
        Err(err) => {
            eprintln!("percept: {err}");
            std::process::exit(1);
        }
    };
    let at_home = home.as_deref() == Some(checkout.as_path());
    if let Some(command) = &cli.command {
        if at_home && needs_project(command) {
            eprintln!("percept: {} is the home level, not a project; run this inside one", checkout.display());
            std::process::exit(1);
        }
    }
    let root = project_of(&checkout);
    let project: Option<PathBuf> = (!at_home).then(|| root.clone());
    let cli_source = crate::core::Source {
        name: CLI_SOURCE_NAME.to_string(),
        path: root.clone(),
    };
    let result = match cli.command {
        // `hook_main` above exits before this match is ever reached.
        Some(Command::Hook(_)) => unreachable!(),
        Some(Command::Search(args)) => {
            open_log(&checkout).and_then(|log| cli::search(args, &log, log.me(), project.as_deref()))
        }
        Some(command @ (Command::Add(_) | Command::Remove(_) | Command::Change(_))) => {
            writer_context(&checkout, &root, project.as_deref(), home.as_deref()).and_then(
                |(schemas, log, me, cause)| {
                    let writer = cli::Writer::new(&log, schemas.as_ref(), &cli_source, me, cause);
                    match command {
                        Command::Add(args) => cli::add(args, &writer, &checkout, at_home),
                        Command::Remove(args) => cli::remove(args, &writer, at_home),
                        Command::Change(args) => cli::change(args, &writer),
                        _ => unreachable!(),
                    }
                },
            )
        }
        Some(Command::Show(args)) => mapstore::load_schemas(project.as_deref(), home.as_deref()).and_then(|schemas| {
            let log = open_log(&checkout)?;
            cli::show(args, &log, &schemas, &root)
        }),
        #[cfg(feature = "lab")]
        Some(Command::Ask(args)) => {
            lab::headless_turn(false, args.prompt, args.yes, cli_source, &checkout, home.as_deref())
                .await
        }
        Some(Command::Init(args)) => open_log(&checkout)
            .and_then(|log| cli::init::run(args, &checkout, &log, &cli_source, home.as_deref())),
        Some(Command::Web) => match open_log(&checkout) {
            Ok(log) => server::run(std::sync::Arc::new(log), cli_source.clone()).await,
            Err(err) => Err(err),
        },
        #[cfg(feature = "lab")]
        Some(Command::Code) => {
            lab::try_main(
                crate::core::Source {
                    name: lab::CODE_SOURCE_NAME.to_string(),
                    path: root,
                },
                &checkout,
                home.as_deref(),
            )
            .await
        }
        None => mapstore::load_schemas(project.as_deref(), home.as_deref())
            .and_then(|schemas| open_log(&checkout).and_then(|log| cli::start(&log, &schemas, &root))),
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
