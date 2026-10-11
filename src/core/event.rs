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
    pub actor: Actor,
    pub payload: Payload,
    pub raw: Option<serde_json::Value>,
    pub created_at: Timestamp,
}

impl Event {
    pub fn session_created(client_session_id: ClientSessionId) -> Self {
        let session = SessionId::new();
        Self::build(Actor::System, Payload::SessionCreated { session, client_session_id }, None)
    }

    pub fn message(
        session: SessionId,
        actor: Actor,
        content: &str,
        raw: serde_json::Value,
    ) -> Self {
        let payload = Payload::Message { session, content: content.into() };
        Self::build(actor, payload, Some(raw))
    }

    pub fn tool_used(session: SessionId, tool: &str, raw: serde_json::Value) -> Self {
        let payload = Payload::ToolUsed { session, tool: tool.into() };
        Self::build(Actor::Agent, payload, Some(raw))
    }

    pub fn session_started(session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(Actor::System, Payload::SessionStarted { session }, Some(raw))
    }

    pub fn session_stopped(session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(Actor::System, Payload::SessionStopped { session }, Some(raw))
    }

    fn build(actor: Actor, payload: Payload, raw: Option<serde_json::Value>) -> Self {
        Self {
            id: EventId::new(),
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
        let event = Event::session_created(ClientSessionId::new("a".into()));

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

        let event = Event::message(session, Actor::Human, "hi", json!({"r": 1}));

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload, Payload::Message { session, content: "hi".into() });
        assert_eq!(event.raw, Some(json!({"r": 1})));
    }

    #[test]
    fn tool_used_is_an_agent_event_carrying_the_tool() {
        let session = SessionId::new();

        let event = Event::tool_used(session, "Bash", json!({"r": 2}));

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::ToolUsed { session, tool: "Bash".into() });
        assert_eq!(event.raw, Some(json!({"r": 2})));
    }

    #[test]
    fn session_started_is_a_system_event() {
        let session = SessionId::new();

        let event = Event::session_started(session, json!({"r": 3}));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::SessionStarted { session });
        assert_eq!(event.raw, Some(json!({"r": 3})));
    }

    #[test]
    fn session_stopped_is_a_system_event() {
        let session = SessionId::new();

        let event = Event::session_stopped(session, json!({"r": 4}));

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
