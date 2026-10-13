use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use super::hook::{Client, hook};
use super::maps::{add, create, show};
use super::status::status;
use crate::app::AppService;
use crate::core::EventStore;
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
    /// Creates a map of the given kind in the current project.
    Create {
        #[arg(allow_hyphen_values = true)]
        kind: String,
    },
    /// Adds a node, `add <map> <kind> <title>`, or an edge, `add <map> <from> <kind> <to>`.
    Add {
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

pub async fn run<S: EventStore + Send + Sync + 'static>(service: Arc<AppService<S>>, home: PathBuf) -> ExitCode {
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
        Some(Command::Create { kind }) => create(&service, &kind).await,
        Some(Command::Add { args }) => add(&service, &args).await,
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
