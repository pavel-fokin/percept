use tokio::io::AsyncReadExt;

use super::Result;
use crate::app::{AppService, source_path};
use crate::core::{Actor, ClientSessionId, Event, EventStore, SessionId, Source};

pub(super) async fn hook<S: EventStore>(service: &AppService<S>, client: String) -> Result<()> {
    let mut input = String::new();
    tokio::io::stdin().read_to_string(&mut input).await?;
    let raw: serde_json::Value = serde_json::from_str(&input)?;
    let (client_session_id, event) = event_from_hook(raw)?;
    let source = Source { client, path: source_path(&std::env::current_dir()?).await };

    service.record(client_session_id, source, event).await
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

}
