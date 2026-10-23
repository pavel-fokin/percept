mod app;
mod cli;
mod core;
mod eventstore;
mod skillstore;
mod server;
mod shared;

use std::process::ExitCode;

use crate::eventstore::JsonlStore;
use crate::skillstore::MarkdownStore;

#[tokio::main]
async fn main() -> ExitCode {
    let Some(home) = std::env::home_dir() else {
        eprintln!("percept: cannot find the home directory");
        return ExitCode::FAILURE;
    };
    let store = JsonlStore::new(home.join(".percept").join("percept.jsonl"));
    cli::run(store, MarkdownStore, home).await
}
