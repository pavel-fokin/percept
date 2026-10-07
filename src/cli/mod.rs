//! Presentation layer: the command-line interface.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tokio::io::AsyncReadExt;

use crate::app::AppService;
use crate::core::{Actor, EventStore};

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
        #[arg(value_parser = ["claude", "codex"])]
        client: String,
    },
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
        Some(Command::Hook { .. }) => hook(service).await,
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

async fn hook<S: EventStore>(service: &AppService<S>) -> Result<()> {
    let mut input = String::new();
    tokio::io::stdin().read_to_string(&mut input).await?;
    let payload: serde_json::Value = serde_json::from_str(&input)?;
    let name = payload["hook_event_name"].as_str();
    service.record(actor_of(name)?, payload).await
}

fn actor_of(hook_event_name: Option<&str>) -> Result<Actor> {
    match hook_event_name {
        Some("UserPromptSubmit") => Ok(Actor::Human),
        Some("PostToolUse" | "SubagentStop" | "Stop") => Ok(Actor::Agent),
        Some("SessionStart") => Ok(Actor::System),
        Some(name) => Err(format!("unknown hook_event_name: {name}").into()),
        None => Err("missing hook_event_name".into()),
    }
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
    fn hook_event_names_map_to_actors() {
        for (name, actor) in [
            ("UserPromptSubmit", Actor::Human),
            ("PostToolUse", Actor::Agent),
            ("SubagentStop", Actor::Agent),
            ("Stop", Actor::Agent),
            ("SessionStart", Actor::System),
        ] {
            assert_eq!(actor_of(Some(name)).unwrap(), actor);
        }
    }

    #[test]
    fn unknown_or_missing_hook_event_name_is_an_error() {
        assert!(actor_of(Some("PreToolUse")).is_err());
        assert!(actor_of(None).is_err());
    }

    #[test]
    fn status_line_phrasings() {
        assert_eq!(status_line(0), "Nothing recorded yet.");
        assert_eq!(status_line(1), "1 event recorded.");
        assert_eq!(status_line(3), "3 events recorded.");
    }
}
