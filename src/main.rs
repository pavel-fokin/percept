mod app;
mod cli;
mod core;
mod eventstore;
mod server;
mod shared;

use std::process::ExitCode;
use std::sync::Arc;

use crate::app::AppService;
use crate::eventstore::JsonlStore;

#[tokio::main]
async fn main() -> ExitCode {
    let Some(home) = std::env::home_dir() else {
        eprintln!("percept: cannot find the home directory");
        return ExitCode::FAILURE;
    };
    let store = JsonlStore::new(home.join(".percept").join("percept.jsonl"));
    cli::run(Arc::new(AppService::new(store))).await
}
