use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use super::hook::{Client, hook};
use super::maps::{add, change, describe, new_board, new_map, remove, show, skills};
use super::projects::init;
use super::status::status;
use crate::app::AppService;
use crate::core::{EventLog, SkillStore};
use crate::server;
use crate::shared::Result;

/// Records what coding agents do.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum New {
    /// Creates a board.
    Board {
        #[arg(allow_hyphen_values = true)]
        path: String,
    },
    /// Creates a map from a skill.
    Map {
        #[arg(allow_hyphen_values = true)]
        path: String,
        #[arg(long)]
        skill: String,
    },
}

#[derive(Subcommand)]
enum Command {
    /// Records a hook payload read from stdin.
    Hook {
        #[arg(value_enum)]
        client: Client,
    },
    /// Creates a project at the root of the current git repository, or in the current folder outside one, named after that folder unless a name is given.
    Init { name: Option<String> },
    /// Creates a board, `new board <path>`, or a map, `new map <path> --skill <skill>`. A <path> is a name in the current project or `/<project>/<board>`; a map's is also `/<project>/<map>` or `/<project>/<board>/<map>`, the last shows the map on that board only. A map is made from the skill of that name in ~/.percept/skills. Boards and maps of a project share names.
    New {
        #[command(subcommand)]
        what: New,
    },
    /// Lists the skills in ~/.percept/skills that a map can be created from.
    Skills,
    /// Adds a node, `add <map> <kind> <title>`, or an edge, `add <map> <from> <kind> <to>`. A <map> is a name in the current project, `/<project>/<map>` or `/<project>/<board>/<map>`; every write prints the path of the map.
    #[command(after_help = "Node options: --key <value> for properties; repeat --ref 'msg:<text>' or --ref 'file:<path>:<from>-<to>' for saved excerpts.")]
    Add {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Changes a node, `change <map> <title> [<new title>]`.
    #[command(after_help = "Options: --key <value> sets a property and keeps the others; repeat --ref 'msg:<text>' or --ref 'file:<path>:<from>-<to>' to add saved excerpts.")]
    Change {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Removes a node, `remove <map> <title>`, or an edge, `remove <map> <from> <kind> <to>`.
    Remove {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Lists a project's maps and boards (`show`, `show /<project>`), a board's maps (`show /<project>/<board>`), or one map's nodes and edges.
    Show { map: Option<String> },
    /// Prints a map's skill as markdown: its purpose, instructions, and node and edge kinds.
    Describe { map: String },
    /// Serves the page in the browser.
    Ui {
        /// Port to serve on; 0 picks a free one.
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Serves without opening the browser.
        #[arg(long)]
        no_open: bool,
    },
}

pub async fn run<L: EventLog + Send + Sync + 'static, M: SkillStore + Clone + Send + Sync + 'static>(log: L, skills: M, home: PathBuf) -> ExitCode {
    // clap exits 2 on a usage error, and exit 2 blocks the agent's action.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() { ExitCode::FAILURE } else { ExitCode::SUCCESS };
        }
    };
    let result = match cli.command {
        Some(Command::Ui { port, no_open }) => server::serve(log, skills, home, port, !no_open).await,
        command => operate(&log, skills, &home, command).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("percept: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Runs one command in one transaction.
async fn operate<L: EventLog, M: SkillStore>(log: &L, skill_store: M, home: &Path, command: Option<Command>) -> Result<()> {
    let service = AppService::new(log.begin().await?, skill_store);
    let service = &service;

    match command {
        None => status(service).await,
        Some(Command::Hook { client }) => hook(service, client).await,
        Some(Command::Init { name }) => init(service, name.as_deref()).await,
        Some(Command::New { what: New::Board { path } }) => new_board(service, &path).await,
        Some(Command::New { what: New::Map { path, skill } }) => new_map(service, home, &path, &skill).await,
        Some(Command::Skills) => skills(service, home).await,
        Some(Command::Add { args }) => add(service, &args).await,
        Some(Command::Change { args }) => change(service, &args).await,
        Some(Command::Remove { args }) => remove(service, &args).await,
        Some(Command::Show { map }) => show(service, map.as_deref()).await,
        Some(Command::Describe { map }) => describe(service, &map).await,
        Some(Command::Ui { .. }) => unreachable!("percept ui begins a transaction per request"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_passes_reference_flags_and_values_to_its_parser() {
        let cli = Cli::try_parse_from([
            "percept", "add", "m", "k", "T", "--ref", "msg:--flag: text", "--note", "-x",
        ]).unwrap();

        let Some(Command::Add { args }) = cli.command else { panic!("expected add") };
        assert_eq!(args, ["m", "k", "T", "--ref", "msg:--flag: text", "--note", "-x"]);
    }

    #[test]
    fn change_passes_its_title_flags_and_values_to_its_parser() {
        let cli = Cli::try_parse_from([
            "percept", "change", "m", "T", "New", "--note", "", "--ref", "msg:text",
        ]).unwrap();

        let Some(Command::Change { args }) = cli.command else { panic!("expected change") };
        assert_eq!(args, ["m", "T", "New", "--note", "", "--ref", "msg:text"]);
    }

    #[test]
    fn remove_passes_a_node_title_to_its_parser() {
        let cli = Cli::try_parse_from(["percept", "remove", "m", "A"]).unwrap();

        let Some(Command::Remove { args }) = cli.command else { panic!("expected remove") };
        assert_eq!(args, ["m", "A"]);
    }

    #[test]
    fn remove_passes_its_titles_and_any_flag_to_its_parser() {
        let cli = Cli::try_parse_from(["percept", "remove", "m", "A", "links", "B", "--ref", "msg:x"]).unwrap();

        let Some(Command::Remove { args }) = cli.command else { panic!("expected remove") };
        assert_eq!(args, ["m", "A", "links", "B", "--ref", "msg:x"]);
    }

    #[test]
    fn new_takes_a_board_or_a_map_with_a_required_skill() {
        let board = Cli::try_parse_from(["percept", "new", "board", "storage"]).unwrap();
        let map = Cli::try_parse_from(["percept", "new", "map", "/p/b/m", "--skill", "decision"]).unwrap();

        assert!(matches!(board.command, Some(Command::New { what: New::Board { path } }) if path == "storage"));
        assert!(matches!(map.command, Some(Command::New { what: New::Map { path, skill } }) if path == "/p/b/m" && skill == "decision"));
        assert!(Cli::try_parse_from(["percept", "new", "map", "m"]).is_err());
        assert!(Cli::try_parse_from(["percept", "new", "project", "p"]).is_err());
        assert!(Cli::try_parse_from(["percept", "create", "m", "decision"]).is_err());
    }
}
