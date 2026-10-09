//! Presentation layer: the command-line interface.

use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::io::AsyncReadExt;

use crate::app::AppService;
use crate::core::{Actor, Event, EventStore};
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
        #[arg(value_parser = ["claude", "codex"])]
        client: String,
    },
    /// Serves the page in the browser.
    Ui,
}

pub async fn run<S: EventStore + Send + Sync + 'static>(service: Arc<AppService<S>>) -> ExitCode {
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
        Some(Command::Hook { .. }) => hook(&service).await,
        Some(Command::Ui) => server::serve(service).await,
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
    let raw: serde_json::Value = serde_json::from_str(&input)?;
    service.record(event_from_hook(raw)?).await
}

fn event_from_hook(raw: serde_json::Value) -> Result<Event> {
    let name = raw["hook_event_name"].as_str();
    let event = match name {
        Some("UserPromptSubmit") => Event::message(Actor::Human, &text(&raw, "prompt")?, raw),
        Some("Stop" | "SubagentStop") => {
            Event::message(Actor::Agent, &text(&raw, "last_assistant_message")?, raw)
        }
        Some("PostToolUse") => Event::tool_used(raw),
        Some("SessionStart") => Event::session_started(raw),
        Some(name) => return Err(format!("unknown hook_event_name: {name}").into()),
        None => return Err("missing hook_event_name".into()),
    };
    Ok(event)
}

fn text(raw: &serde_json::Value, field: &str) -> Result<String> {
    raw[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("missing {field}").into())
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
    use crate::core::Kind;
    use serde_json::json;

    #[test]
    fn user_prompt_is_a_human_message() {
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": "hi"});

        let event = event_from_hook(raw.clone()).unwrap();

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.kind, Kind::Message);
        assert_eq!(event.payload, json!({"content": "hi"}));
        assert_eq!(event.raw, raw);
    }

    #[test]
    fn stops_are_agent_messages() {
        for name in ["Stop", "SubagentStop"] {
            let raw = json!({"hook_event_name": name, "last_assistant_message": "done"});

            let event = event_from_hook(raw.clone()).unwrap();

            assert_eq!(event.actor, Actor::Agent);
            assert_eq!(event.kind, Kind::Message);
            assert_eq!(event.payload, json!({"content": "done"}));
            assert_eq!(event.raw, raw);
        }
    }

    #[test]
    fn post_tool_use_is_a_tool_used_event() {
        let raw = json!({"hook_event_name": "PostToolUse", "tool_name": "Bash"});

        let event = event_from_hook(raw.clone()).unwrap();

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.kind, Kind::ToolUsed);
        assert_eq!(event.payload, json!({}));
        assert_eq!(event.raw, raw);
    }

    #[test]
    fn session_start_is_a_session_started_event() {
        let raw = json!({"hook_event_name": "SessionStart"});

        let event = event_from_hook(raw.clone()).unwrap();

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.kind, Kind::SessionStarted);
        assert_eq!(event.payload, json!({}));
        assert_eq!(event.raw, raw);
    }

    #[test]
    fn unknown_or_missing_hook_event_name_is_an_error() {
        assert!(event_from_hook(json!({"hook_event_name": "PreToolUse"})).is_err());
        assert!(event_from_hook(json!({})).is_err());
    }

    #[test]
    fn missing_or_non_string_content_is_an_error() {
        assert!(event_from_hook(json!({"hook_event_name": "UserPromptSubmit"})).is_err());
        assert!(event_from_hook(json!({"hook_event_name": "Stop", "last_assistant_message": 1})).is_err());
    }

    #[test]
    fn status_line_phrasings() {
        assert_eq!(status_line(0), "Nothing recorded yet.");
        assert_eq!(status_line(1), "1 event recorded.");
        assert_eq!(status_line(3), "3 events recorded.");
    }
}
