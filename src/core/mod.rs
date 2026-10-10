//! Domain layer: `Event` and the store trait that persists it.

use std::error::Error;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::shared::Id;

pub type EventId = Id<Event>;

/// A conversation with a coding client, created by a `SessionCreated` event.
#[derive(Serialize)]
pub struct Session {
    pub id: SessionId,
    pub key: SessionKey,
    pub created_at: Timestamp,
    /// The first thing the human said, if they said anything.
    pub title: Option<String>,
}

pub type SessionId = Id<Session>;

/// The client's own session identifier, as the client sent it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionKey(String);

impl SessionKey {
    pub fn new(key: String) -> Self {
        Self(key)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Who an event is attributed to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Human,
    Agent,
    System,
}

/// What an event means, whichever client produced it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Message,
    ToolUsed,
    SessionCreated,
    SessionStarted,
    SessionStopped,
}

/// `payload` holds the kind's domain data; `raw` is the hook input, unchanged.
/// Events percept makes itself have no `raw`.
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub session: SessionId,
    pub actor: Actor,
    pub kind: Kind,
    pub payload: serde_json::Value,
    pub raw: Option<serde_json::Value>,
    pub created_at: Timestamp,
}

impl Event {
    pub fn session_created(key: SessionKey) -> Self {
        Self::build(
            SessionId::new(),
            Actor::System,
            Kind::SessionCreated,
            json!({ "key": key.as_str() }),
            None,
        )
    }

    pub fn message(
        session: SessionId,
        actor: Actor,
        content: &str,
        raw: serde_json::Value,
    ) -> Self {
        Self::build(session, actor, Kind::Message, json!({ "content": content }), Some(raw))
    }

    pub fn tool_used(session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(session, Actor::Agent, Kind::ToolUsed, json!({}), Some(raw))
    }

    pub fn session_started(session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(session, Actor::System, Kind::SessionStarted, json!({}), Some(raw))
    }

    pub fn session_stopped(session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(session, Actor::System, Kind::SessionStopped, json!({}), Some(raw))
    }

    fn build(
        session: SessionId,
        actor: Actor,
        kind: Kind,
        payload: serde_json::Value,
        raw: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: EventId::new(),
            session,
            actor,
            kind,
            payload,
            raw,
            created_at: Timestamp::now(),
        }
    }
}

pub trait EventStore {
    /// Held while recording; dropping it releases the lock.
    type Lock: Send;

    fn lock(&self) -> impl Future<Output = Result<Self::Lock, Box<dyn Error + Send + Sync>>> + Send;

    fn session(
        &self,
        key: &SessionKey,
    ) -> impl Future<Output = Result<Option<SessionId>, Box<dyn Error + Send + Sync>>> + Send;

    fn all(&self) -> impl Future<Output = Result<Vec<Event>, Box<dyn Error + Send + Sync>>> + Send;

    fn append(
        &self,
        event: &Event,
    ) -> impl Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_created_is_a_system_event_carrying_the_key() {
        let event = Event::session_created(SessionKey::new("a".into()));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.kind, Kind::SessionCreated);
        assert_eq!(event.payload, json!({"key": "a"}));
        assert_eq!(event.raw, None);
    }

    #[test]
    fn message_carries_the_session_actor_and_content() {
        let session = SessionId::new();

        let event = Event::message(session, Actor::Human, "hi", json!({"r": 1}));

        assert_eq!(event.session, session);
        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.kind, Kind::Message);
        assert_eq!(event.payload, json!({"content": "hi"}));
        assert_eq!(event.raw, Some(json!({"r": 1})));
    }

    #[test]
    fn tool_used_is_an_agent_event_with_an_empty_payload() {
        let session = SessionId::new();

        let event = Event::tool_used(session, json!({"r": 2}));

        assert_eq!(event.session, session);
        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.kind, Kind::ToolUsed);
        assert_eq!(event.payload, json!({}));
        assert_eq!(event.raw, Some(json!({"r": 2})));
    }

    #[test]
    fn session_started_is_a_system_event_with_an_empty_payload() {
        let session = SessionId::new();

        let event = Event::session_started(session, json!({"r": 3}));

        assert_eq!(event.session, session);
        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.kind, Kind::SessionStarted);
        assert_eq!(event.payload, json!({}));
        assert_eq!(event.raw, Some(json!({"r": 3})));
    }

    #[test]
    fn session_stopped_is_a_system_event_with_an_empty_payload() {
        let session = SessionId::new();

        let event = Event::session_stopped(session, json!({"r": 4}));

        assert_eq!(event.session, session);
        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.kind, Kind::SessionStopped);
        assert_eq!(event.payload, json!({}));
        assert_eq!(event.raw, Some(json!({"r": 4})));
    }

    #[test]
    fn serializes_as_a_flat_line() {
        let event = Event::message(SessionId::new(), Actor::Human, "hi", json!({"r": 1}));

        let line = serde_json::to_value(&event).unwrap();

        assert_eq!(line["session"], serde_json::to_value(event.session).unwrap());
        assert_eq!(line["actor"], "human");
        assert_eq!(line["kind"], "Message");
        assert_eq!(line["payload"], json!({"content": "hi"}));
        assert_eq!(line["raw"], json!({"r": 1}));
    }
}
