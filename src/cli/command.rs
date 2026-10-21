use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use super::hook::{Client, hook};
use super::maps::{add, change, create, remove, show};
use super::projects::init;
use super::status::status;
use crate::app::AppService;
use crate::core::{DefinitionStore, EventStore};
use crate::server;

/// Records what coding agents do.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
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
    /// Creates a map of the given kind in the current project, defined by the given TOML file.
    Create {
        #[arg(allow_hyphen_values = true)]
        kind: String,
        file: PathBuf,
    },
    /// Adds a node, `add <map> <kind> <title>`, or an edge, `add <map> <from> <kind> <to>`.
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
    /// Lists the current project's maps, or one map's nodes and edges.
    Show { map: Option<String> },
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

pub async fn run<S: EventStore + Send + Sync + 'static, M: DefinitionStore + Send + Sync + 'static>(service: Arc<AppService<S, M>>, home: PathBuf) -> ExitCode {
    // clap exits 2 on a usage error, and exit 2 blocks the agent's action.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() { ExitCode::FAILURE } else { ExitCode::SUCCESS };
        }
    };
    let result = match cli.command {
        None => status(&service).await,
        Some(Command::Hook { client }) => hook(&service, client).await,
        Some(Command::Init { name }) => init(&service, name.as_deref()).await,
        Some(Command::Create { kind, file }) => create(&service, &kind, &file).await,
        Some(Command::Add { args }) => add(&service, &args).await,
        Some(Command::Change { args }) => change(&service, &args).await,
        Some(Command::Remove { args }) => remove(&service, &args).await,
        Some(Command::Show { map }) => show(&service, map.as_deref()).await,
        Some(Command::Ui { port, no_open }) => server::serve(service, home, port, !no_open).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("percept: {error}");
            ExitCode::FAILURE
        }
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
}
