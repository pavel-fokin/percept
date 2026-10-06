//! Presentation layer: the command-line interface.

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use tokio::io::AsyncReadExt;

use crate::app::AppService;
use crate::core::{Client, EventStore};

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
    Hook { client: ClientArg },
}

#[derive(Clone, Copy, ValueEnum)]
enum ClientArg {
    Claude,
    Codex,
}

impl From<ClientArg> for Client {
    fn from(arg: ClientArg) -> Self {
        match arg {
            ClientArg::Claude => Client::Claude,
            ClientArg::Codex => Client::Codex,
        }
    }
}

pub async fn run<S: EventStore>(service: &AppService<S>) -> ExitCode {
    // clap exits 2 on a usage error, and exit 2 blocks the agent's action.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() { ExitCode::FAILURE } else { ExitCode::SUCCESS };
        }
    };
    let result = match cli.command {
        None => status(service).await,
        Some(Command::Hook { client }) => hook(service, client.into()).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("percept: {error}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn status<S: EventStore>(service: &AppService<S>) -> Result<()> {
    println!("percept • {}", status_line(service.event_count().await?));
    Ok(())
}

async fn hook<S: EventStore>(service: &AppService<S>, client: Client) -> Result<()> {
    let mut input = String::new();
    tokio::io::stdin().read_to_string(&mut input).await?;
    service.record(client, serde_json::from_str(&input)?).await
}

fn status_line(count: usize) -> String {
    match count {
        0 => "Nothing recorded yet.".to_string(),
        1 => "1 event recorded.".to_string(),
        n => format!("{n} events recorded."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_phrasings() {
        assert_eq!(status_line(0), "Nothing recorded yet.");
        assert_eq!(status_line(1), "1 event recorded.");
        assert_eq!(status_line(3), "3 events recorded.");
    }
}
