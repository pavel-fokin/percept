mod app;
mod cli;
mod core;
mod eventstore;
mod shared;

use std::process::ExitCode;

use crate::app::AppService;
use crate::eventstore::JsonlStore;

#[tokio::main]
async fn main() -> ExitCode {
    let Some(home) = std::env::home_dir() else {
        eprintln!("percept: cannot find the home directory");
        return ExitCode::FAILURE;
    };
    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("percept: {error}");
            return ExitCode::FAILURE;
        }
    };
    let store = JsonlStore::new(home.join(".percept").join("percept.jsonl"));
    cli::run(&AppService::new(store), &current_dir, &home).await
}
