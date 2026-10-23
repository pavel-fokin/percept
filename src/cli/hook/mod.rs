mod claude;
mod codex;

use clap::ValueEnum;
use tokio::io::AsyncReadExt;

use crate::app::{AppService, source_path};
use crate::core::{Actor, ClientSessionId, Event, EventStore, SessionId, SkillStore, Source};
use crate::shared::Result;

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub(super) enum Client {
    Claude,
    Codex,
}

impl Client {
    pub(super) fn name(self) -> &'static str {
        match self {
            Client::Claude => "claude-code",
            Client::Codex => "codex",
        }
    }
}

/// What a client's hook payload means to percept.
enum Translation {
    Record(ClientSessionId, EventBuilder),
    Skip,
}

pub(super) async fn hook<S: EventStore, M: SkillStore>(service: &AppService<S, M>, client: Client) -> Result<()> {
    let mut input = String::new();
    tokio::io::stdin().read_to_string(&mut input).await?;
    let raw: serde_json::Value = serde_json::from_str(&input)?;
    let translation = match client {
        Client::Claude => claude::translate(raw)?,
        Client::Codex => codex::translate(raw)?,
    };
    let Translation::Record(client_session_id, event) = translation else {
        return Ok(());
    };
    let source = Source::Checkout { client: client.name().to_owned(), path: source_path(&std::env::current_dir()?).await };

    service.record(client_session_id, source, event).await
}

/// Makes the hook's event once its session is known.
type EventBuilder = Box<dyn FnOnce(Source, SessionId) -> Event>;

/// A client's reading of a tool call: the input and output worth recording, from the tool's name and the hook.
type ToolIo = fn(&str, &serde_json::Value) -> (Option<String>, Option<String>);

/// Parses the hooks that Claude and Codex send in the same shape.
fn shared(raw: serde_json::Value, tool_io: ToolIo) -> Result<Translation> {
    let client_session_id = session_id(&raw)?;

    let name = raw["hook_event_name"].as_str();
    let event: EventBuilder = match name {
        Some("UserPromptSubmit") => {
            let content = text(&raw, "prompt")?;
            Box::new(move |source, s| Event::message(source, s, Actor::Human, &content, raw))
        }
        Some("Stop") => {
            let content = text(&raw, "last_assistant_message")?;
            Box::new(move |source, s| Event::message(source, s, Actor::Agent, &content, raw))
        }
        Some("PostToolUse") => {
            let tool = text(&raw, "tool_name")?;
            let (input, output) = tool_io(&tool, &raw);
            Box::new(move |source, s| Event::tool_used(source, s, &tool, input, output, raw))
        }
        Some("SessionStart") => Box::new(move |source, s| Event::session_started(source, s, raw)),
        Some("SessionEnd") => Box::new(move |source, s| Event::session_stopped(source, s, raw)),
        Some(name) => return Err(format!("unknown hook_event_name: {name}").into()),
        None => return Err("missing hook_event_name".into()),
    };

    Ok(Translation::Record(client_session_id, event))
}

fn session_id(raw: &serde_json::Value) -> Result<ClientSessionId> {
    Ok(ClientSessionId::new(text(raw, "session_id")?))
}

fn text(raw: &serde_json::Value, field: &str) -> Result<String> {
    raw[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("missing {field}").into())
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_owned)
}

fn str_field(value: &serde_json::Value, field: &str) -> Option<String> {
    non_empty(value[field].as_str())
}

fn joined(lines: Vec<&str>) -> Option<String> {
    let text = lines.join("\n");
    (!text.is_empty()).then_some(text)
}

/// Null, `{}` and `[]` carry nothing worth showing.
fn is_blank(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Object(fields) => fields.is_empty(),
        serde_json::Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

/// The tool's input as compact JSON, for a tool whose arguments have no one field to show.
fn json_input(raw: &serde_json::Value) -> Option<String> {
    let input = &raw["tool_input"];
    (!is_blank(input)).then(|| input.to_string())
}

/// The text parts of an MCP result: Codex wraps them in `content`, Claude sends the array bare.
fn mcp_output(response: &serde_json::Value) -> Option<String> {
    let parts = response["content"].as_array().or(response.as_array())?;
    text_parts(parts)
}

/// The `text` of each part that has one, joined; images and other parts carry none.
fn text_parts(parts: &[serde_json::Value]) -> Option<String> {
    joined(parts.iter().filter_map(|part| part["text"].as_str()).collect())
}

/// Reads a tool no client table covers: the input as compact JSON, the response as text.
fn unknown_tool_io(raw: &serde_json::Value) -> (Option<String>, Option<String>) {
    let output = match &raw["tool_response"] {
        response if is_blank(response) => None,
        serde_json::Value::String(text) => non_empty(Some(text)),
        serde_json::Value::Array(items) if items.iter().all(|item| item["text"].is_string()) => text_parts(items),
        response => serde_json::to_string_pretty(response).ok(),
    };

    (json_input(raw), output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Payload;
    use serde_json::json;

    fn build(raw: serde_json::Value) -> (ClientSessionId, SessionId, Event) {
        record(shared_no_io(raw).unwrap())
    }

    fn shared_no_io(raw: serde_json::Value) -> Result<Translation> {
        shared(raw, |_, _| (None, None))
    }

    /// A client's `tool_io` applied to a hook that names its tool.
    pub(super) fn io(tool_io: ToolIo, raw: serde_json::Value) -> (Option<String>, Option<String>) {
        tool_io(raw["tool_name"].as_str().unwrap(), &raw)
    }

    pub(super) fn some(text: &str) -> Option<String> {
        Some(text.to_owned())
    }

    pub(super) fn record(translation: Translation) -> (ClientSessionId, SessionId, Event) {
        let Translation::Record(client_session_id, event) = translation else {
            panic!("expected a record");
        };
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
    fn stop_is_an_agent_message() {
        let raw = json!({"hook_event_name": "Stop", "last_assistant_message": "done", "session_id": "a"});

        let (_, session, event) = build(raw.clone());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::Message { session, content: "done".into() });
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn post_tool_use_is_a_tool_used_event() {
        let raw = json!({"hook_event_name": "PostToolUse", "tool_name": "Bash", "session_id": "a"});

        let (_, session, event) = build(raw.clone());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::ToolUsed { session, tool: "Bash".into(), input: None, output: None });
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
        assert!(shared_no_io(json!({"hook_event_name": "PreToolUse", "session_id": "a"})).is_err());
        assert!(shared_no_io(json!({"hook_event_name": "SubagentStop", "session_id": "a"})).is_err());
        assert!(shared_no_io(json!({"session_id": "a"})).is_err());
    }

    #[test]
    fn missing_or_non_string_session_id_is_an_error() {
        assert!(shared_no_io(json!({"hook_event_name": "PostToolUse"})).is_err());
        assert!(shared_no_io(json!({"hook_event_name": "PostToolUse", "session_id": 1})).is_err());
    }

    #[test]
    fn missing_or_non_string_content_is_an_error() {
        assert!(shared_no_io(json!({"hook_event_name": "UserPromptSubmit", "session_id": "a"})).is_err());
        assert!(shared_no_io(json!({"hook_event_name": "Stop", "last_assistant_message": 1, "session_id": "a"})).is_err());
    }

    #[test]
    fn missing_tool_name_is_an_error() {
        assert!(shared_no_io(json!({"hook_event_name": "PostToolUse", "session_id": "a"})).is_err());
    }
}
