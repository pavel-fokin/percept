//! Domain layer: `Event` and the store trait that persists it.

use std::error::Error;

use serde::{Deserialize, Serialize};
use serde_json::json;

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

/// What an event means, whichever client produced it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Message,
    ToolUsed,
    SessionStarted,
}

/// `payload` holds the kind's domain data; `raw` is the hook input, unchanged.
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub actor: Actor,
    pub kind: Kind,
    pub payload: serde_json::Value,
    pub raw: serde_json::Value,
}

impl Event {
    pub fn message(actor: Actor, content: &str, raw: serde_json::Value) -> Self {
        Self::build(actor, Kind::Message, json!({ "content": content }), raw)
    }

    pub fn tool_used(raw: serde_json::Value) -> Self {
        Self::build(Actor::Agent, Kind::ToolUsed, json!({}), raw)
    }

    pub fn session_started(raw: serde_json::Value) -> Self {
        Self::build(Actor::System, Kind::SessionStarted, json!({}), raw)
    }

    fn build(
        actor: Actor,
        kind: Kind,
        payload: serde_json::Value,
        raw: serde_json::Value,
    ) -> Self {
        Self {
            id: EventId::new(),
            actor,
            kind,
            payload,
            raw,
        }
    }
}

pub trait EventStore {
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
    fn message_carries_the_actor_and_content() {
        let event = Event::message(Actor::Human, "hi", json!({"r": 1}));

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.kind, Kind::Message);
        assert_eq!(event.payload, json!({"content": "hi"}));
        assert_eq!(event.raw, json!({"r": 1}));
    }

    #[test]
    fn tool_used_is_an_agent_event_with_an_empty_payload() {
        let event = Event::tool_used(json!({"r": 2}));

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.kind, Kind::ToolUsed);
        assert_eq!(event.payload, json!({}));
        assert_eq!(event.raw, json!({"r": 2}));
    }

    #[test]
    fn session_started_is_a_system_event_with_an_empty_payload() {
        let event = Event::session_started(json!({"r": 3}));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.kind, Kind::SessionStarted);
        assert_eq!(event.payload, json!({}));
        assert_eq!(event.raw, json!({"r": 3}));
    }

    #[test]
    fn serializes_as_a_flat_line() {
        let event = Event::message(Actor::Human, "hi", json!({"r": 1}));

        let line = serde_json::to_value(&event).unwrap();

        assert_eq!(line["actor"], "human");
        assert_eq!(line["kind"], "Message");
        assert_eq!(line["payload"], json!({"content": "hi"}));
        assert_eq!(line["raw"], json!({"r": 1}));
    }
}
