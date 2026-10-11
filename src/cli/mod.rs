//! Presentation layer: the command-line interface.

use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::io::AsyncReadExt;

use std::collections::BTreeMap;

use crate::app::{AppService, source_path};
use crate::core::{Actor, ClientSessionId, Event, EventStore, SessionId, Source};
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
    /// Creates a map of the given kind in the current project.
    Create {
        #[arg(allow_hyphen_values = true)]
        kind: String,
        #[arg(allow_hyphen_values = true)]
        title: String,
    },
    /// Adds a node, `add <map> <kind> <title>`, or an edge, `add <map> <from> <kind> <to>`.
    Add {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Lists the current project's maps, or one map's nodes and edges.
    Show { map: Option<String> },
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
        Some(Command::Hook { client }) => hook(&service, client).await,
        Some(Command::Create { kind, title }) => create(&service, &kind, &title).await,
        Some(Command::Add { args }) => add(&service, &args).await,
        Some(Command::Show { map }) => show(&service, map.as_deref()).await,
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

async fn hook<S: EventStore>(service: &AppService<S>, client: String) -> Result<()> {
    let mut input = String::new();
    tokio::io::stdin().read_to_string(&mut input).await?;
    let raw: serde_json::Value = serde_json::from_str(&input)?;
    let (client_session_id, event) = event_from_hook(raw)?;
    let source = Source { client, path: source_path(&std::env::current_dir()?).await };

    service.record(client_session_id, source, event).await
}

async fn create<S: EventStore>(service: &AppService<S>, kind: &str, title: &str) -> Result<()> {
    let (actor, source) = cli_writer().await?;

    service.create_map(source, actor, kind, title).await
}

async fn add<S: EventStore>(service: &AppService<S>, args: &[String]) -> Result<()> {
    match args {
        [map, kind, title] => {
            let (actor, source) = cli_writer().await?;
            service.add_node(source, actor, map, kind, title).await
        }
        [map, from, kind, to] => {
            let (actor, source) = cli_writer().await?;
            service.add_edge(source, actor, map, from, kind, to).await
        }
        _ => Err("usage: percept add <map> <kind> <title> | percept add <map> <from> <kind> <to>".into()),
    }
}

async fn cli_writer() -> Result<(Actor, Source)> {
    let (actor, client) = writer(env_flag("CLAUDECODE"), env_flag("CODEX_SESSION_ID"))?;
    let source = Source { client: client.into(), path: source_path(&std::env::current_dir()?).await };

    Ok((actor, source))
}

async fn show<S: EventStore>(service: &AppService<S>, map: Option<&str>) -> Result<()> {
    let project = source_path(&std::env::current_dir()?).await;

    match map {
        Some(kind) => {
            let map = service.map(&project, kind).await?;
            let nodes = service.nodes(map.id).await?;
            let edges = service.edges(map.id).await?;

            println!("{}  {}", map.kind, map.title);

            if !nodes.is_empty() {
                println!();
            }
            for node in &nodes {
                println!("{}  {}", node.kind, node.title);
            }

            let titles: BTreeMap<_, _> = nodes.iter().map(|n| (n.id, n.title.as_str())).collect();
            let title = |id| titles.get(&id).copied().ok_or("an edge names a node that is not in its map");
            if !edges.is_empty() {
                println!();
            }
            for edge in edges {
                println!("\"{}\"  {}  \"{}\"", title(edge.from)?, edge.kind, title(edge.to)?);
            }
        }
        None => {
            for s in service.map_summaries(&project).await? {
                println!("{}  {}  {}, {}", s.map.kind, s.map.title, node_count(s.nodes), edge_count(s.edges));
            }
        }
    }

    Ok(())
}

fn node_count(count: usize) -> String {
    match count {
        1 => "1 node".to_string(),
        n => format!("{n} nodes"),
    }
}

fn edge_count(count: usize) -> String {
    match count {
        1 => "1 edge".to_string(),
        n => format!("{n} edges"),
    }
}

fn env_flag(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Who is running the CLI, from the values of `CLAUDECODE` and `CODEX_SESSION_ID`.
fn writer(claudecode: Option<String>, codex_session_id: Option<String>) -> Result<(Actor, &'static str)> {
    match (claudecode.as_deref() == Some("1"), codex_session_id.is_some()) {
        (true, true) => Err("both CLAUDECODE and CODEX_SESSION_ID are set".into()),
        (true, false) => Ok((Actor::Agent, "claude")),
        (false, true) => Ok((Actor::Agent, "codex")),
        (false, false) => Ok((Actor::Human, "percept-cli")),
    }
}

/// Makes the hook's event once its session is known.
type EventBuilder = Box<dyn FnOnce(Source, SessionId) -> Event>;

fn event_from_hook(raw: serde_json::Value) -> Result<(ClientSessionId, EventBuilder)> {
    let client_session_id = ClientSessionId::new(text(&raw, "session_id")?);

    let name = raw["hook_event_name"].as_str();
    let event: EventBuilder = match name {
        Some("UserPromptSubmit") => {
            let content = text(&raw, "prompt")?;
            Box::new(move |source, s| Event::message(source, s, Actor::Human, &content, raw))
        }
        Some("Stop" | "SubagentStop") => {
            let content = text(&raw, "last_assistant_message")?;
            Box::new(move |source, s| Event::message(source, s, Actor::Agent, &content, raw))
        }
        Some("PostToolUse") => {
            let tool = text(&raw, "tool_name")?;
            Box::new(move |source, s| Event::tool_used(source, s, &tool, raw))
        }
        Some("SessionStart") => Box::new(move |source, s| Event::session_started(source, s, raw)),
        Some("SessionEnd") => Box::new(move |source, s| Event::session_stopped(source, s, raw)),
        Some(name) => return Err(format!("unknown hook_event_name: {name}").into()),
        None => return Err("missing hook_event_name".into()),
    };

    Ok((client_session_id, event))
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
    use crate::core::Payload;
    use serde_json::json;

    fn build(raw: serde_json::Value) -> (ClientSessionId, SessionId, Event) {
        let (client_session_id, event) = event_from_hook(raw).unwrap();
        let session = SessionId::new();
        (client_session_id, session, event(Source::fixture(), session))
    }

    #[test]
    fn user_prompt_is_a_human_message() {
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": "hi", "session_id": "a"});

        let (client_session_id, session, event) = build(raw.clone());

        assert_eq!(client_session_id, ClientSessionId::new("a".into()));
        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload, Payload::Message { session, content: "hi".into() });
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn stops_are_agent_messages() {
        for name in ["Stop", "SubagentStop"] {
            let raw = json!({"hook_event_name": name, "last_assistant_message": "done", "session_id": "a"});

            let (_, session, event) = build(raw.clone());

            assert_eq!(event.actor, Actor::Agent);
            assert_eq!(event.payload, Payload::Message { session, content: "done".into() });
            assert_eq!(event.raw, Some(raw));
        }
    }

    #[test]
    fn post_tool_use_is_a_tool_used_event() {
        let raw = json!({"hook_event_name": "PostToolUse", "tool_name": "Bash", "session_id": "a"});

        let (_, session, event) = build(raw.clone());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::ToolUsed { session, tool: "Bash".into() });
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn session_start_is_a_session_started_event() {
        let raw = json!({"hook_event_name": "SessionStart", "session_id": "a"});

        let (_, session, event) = build(raw.clone());

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::SessionStarted { session });
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn session_end_is_a_session_stopped_event() {
        let raw = json!({"hook_event_name": "SessionEnd", "session_id": "a"});

        let (_, session, event) = build(raw.clone());

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::SessionStopped { session });
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn unknown_or_missing_hook_event_name_is_an_error() {
        assert!(event_from_hook(json!({"hook_event_name": "PreToolUse", "session_id": "a"})).is_err());
        assert!(event_from_hook(json!({"session_id": "a"})).is_err());
    }

    #[test]
    fn missing_or_non_string_session_id_is_an_error() {
        assert!(event_from_hook(json!({"hook_event_name": "PostToolUse"})).is_err());
        assert!(event_from_hook(json!({"hook_event_name": "PostToolUse", "session_id": 1})).is_err());
    }

    #[test]
    fn missing_or_non_string_content_is_an_error() {
        assert!(event_from_hook(json!({"hook_event_name": "UserPromptSubmit", "session_id": "a"})).is_err());
        assert!(event_from_hook(json!({"hook_event_name": "Stop", "last_assistant_message": 1, "session_id": "a"})).is_err());
    }

    #[test]
    fn missing_tool_name_is_an_error() {
        assert!(event_from_hook(json!({"hook_event_name": "PostToolUse", "session_id": "a"})).is_err());
    }

    #[test]
    fn writer_follows_the_client_environment() {
        let set = || Some("1".to_string());

        assert_eq!(writer(set(), None).unwrap(), (Actor::Agent, "claude"));
        assert_eq!(writer(None, set()).unwrap(), (Actor::Agent, "codex"));
        assert_eq!(writer(None, None).unwrap(), (Actor::Human, "percept-cli"));
        assert!(writer(set(), set()).is_err());
    }

    #[test]
    fn node_count_phrasings() {
        assert_eq!(node_count(0), "0 nodes");
        assert_eq!(node_count(1), "1 node");
        assert_eq!(node_count(2), "2 nodes");
    }

    #[test]
    fn edge_count_phrasings() {
        assert_eq!(edge_count(0), "0 edges");
        assert_eq!(edge_count(1), "1 edge");
        assert_eq!(edge_count(2), "2 edges");
    }

    #[test]
    fn status_line_phrasings() {
        assert_eq!(status_line(0), "Nothing recorded yet.");
        assert_eq!(status_line(1), "1 event recorded.");
        assert_eq!(status_line(3), "3 events recorded.");
    }
}
