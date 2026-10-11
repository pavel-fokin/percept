use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{ClientSessionId, SessionId};
use crate::shared::Id;

pub type EventId = Id<Event>;

/// Who an event is attributed to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Human,
    Agent,
    System,
}

/// Where an event came from: the coding client and the folder it ran in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub client: String,
    pub path: PathBuf,
}

#[cfg(test)]
impl Source {
    pub fn fixture() -> Self {
        Self { client: "claude".into(), path: "/repo".into() }
    }
}

/// What happened, with the data that kind of event carries.
#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
    SessionCreated { session: SessionId, client_session_id: ClientSessionId },
    SessionStarted { session: SessionId },
    SessionStopped { session: SessionId },
    Message { session: SessionId, content: String },
    ToolUsed { session: SessionId, tool: String },
}

impl Payload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "session.created",
            Self::SessionStarted { .. } => "session.started",
            Self::SessionStopped { .. } => "session.stopped",
            Self::Message { .. } => "message",
            Self::ToolUsed { .. } => "tool.used",
        }
    }

    pub fn session(&self) -> SessionId {
        match self {
            Self::SessionCreated { session, .. }
            | Self::SessionStarted { session }
            | Self::SessionStopped { session }
            | Self::Message { session, .. }
            | Self::ToolUsed { session, .. } => *session,
        }
    }
}

/// `raw` is the hook input, unchanged. Events percept makes itself have no `raw`.
#[derive(Debug, Clone)]
pub struct Event {
    pub id: EventId,
    pub source: Source,
    pub actor: Actor,
    pub payload: Payload,
    pub raw: Option<serde_json::Value>,
    pub created_at: Timestamp,
}

impl Event {
    pub fn session_created(source: Source, session: SessionId, client_session_id: ClientSessionId) -> Self {
        Self::build(source, Actor::System, Payload::SessionCreated { session, client_session_id }, None)
    }

    pub fn message(
        source: Source,
        session: SessionId,
        actor: Actor,
        content: &str,
        raw: serde_json::Value,
    ) -> Self {
        let payload = Payload::Message { session, content: content.into() };
        Self::build(source, actor, payload, Some(raw))
    }

    pub fn tool_used(source: Source, session: SessionId, tool: &str, raw: serde_json::Value) -> Self {
        let payload = Payload::ToolUsed { session, tool: tool.into() };
        Self::build(source, Actor::Agent, payload, Some(raw))
    }

    pub fn session_started(source: Source, session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(source, Actor::System, Payload::SessionStarted { session }, Some(raw))
    }

    pub fn session_stopped(source: Source, session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(source, Actor::System, Payload::SessionStopped { session }, Some(raw))
    }

    fn build(source: Source, actor: Actor, payload: Payload, raw: Option<serde_json::Value>) -> Self {
        Self {
            id: EventId::new(),
            source,
            actor,
            payload,
            raw,
            created_at: Timestamp::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_created_is_a_system_event_carrying_the_client_session_id() {
        let event = Event::session_created(Source::fixture(), SessionId::new(), ClientSessionId::new("a".into()));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload.kind(), "session.created");
        assert!(matches!(
            &event.payload,
            Payload::SessionCreated { client_session_id, .. } if client_session_id.as_str() == "a"
        ));
        assert_eq!(event.raw, None);
    }

    #[test]
    fn message_carries_the_session_actor_and_content() {
        let session = SessionId::new();

        let event = Event::message(Source::fixture(), session, Actor::Human, "hi", json!({"r": 1}));

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload, Payload::Message { session, content: "hi".into() });
        assert_eq!(event.raw, Some(json!({"r": 1})));
    }

    #[test]
    fn tool_used_is_an_agent_event_carrying_the_tool() {
        let session = SessionId::new();

        let event = Event::tool_used(Source::fixture(), session, "Bash", json!({"r": 2}));

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::ToolUsed { session, tool: "Bash".into() });
        assert_eq!(event.raw, Some(json!({"r": 2})));
    }

    #[test]
    fn session_started_is_a_system_event() {
        let session = SessionId::new();

        let event = Event::session_started(Source::fixture(), session, json!({"r": 3}));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::SessionStarted { session });
        assert_eq!(event.raw, Some(json!({"r": 3})));
    }

    #[test]
    fn session_stopped_is_a_system_event() {
        let session = SessionId::new();

        let event = Event::session_stopped(Source::fixture(), session, json!({"r": 4}));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::SessionStopped { session });
        assert_eq!(event.raw, Some(json!({"r": 4})));
    }

    #[test]
    fn every_payload_names_its_kind_and_session() {
        let session = SessionId::new();
        let payloads = [
            (Payload::SessionCreated { session, client_session_id: ClientSessionId::new("a".into()) }, "session.created"),
            (Payload::SessionStarted { session }, "session.started"),
            (Payload::SessionStopped { session }, "session.stopped"),
            (Payload::Message { session, content: "hi".into() }, "message"),
            (Payload::ToolUsed { session, tool: "Bash".into() }, "tool.used"),
        ];

        for (payload, kind) in payloads {
            assert_eq!(payload.kind(), kind);
            assert_eq!(payload.session(), session);
        }
    }
}
