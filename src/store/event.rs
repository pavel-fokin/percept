use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::percept::{self, Actor, EventId, Payload};
use crate::shared::Timestamp;
use crate::store::Error;

/// A `percept::Event` as it travels over the wire. Flat JSON:
/// `{ id, seq, actor, type, causation_id, created_at, payload }`.
/// `payload` shape depends on `type`.
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub seq: u64,
    pub actor: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub causation_id: Option<String>,
    pub created_at: String,
    pub payload: Value,
}

#[derive(Serialize, Deserialize)]
struct MessageBody {
    content: String,
}

impl From<&percept::Event> for Event {
    fn from(event: &percept::Event) -> Self {
        let (kind, payload) = match event.payload() {
            Payload::MessageReceived { content } => (
                "message.received",
                serde_json::to_value(MessageBody {
                    content: content.clone(),
                })
                .expect("MessageBody always serializes"),
            ),
        };

        Self {
            id: event.id().as_uuid().to_string(),
            seq: event.seq(),
            actor: actor_str(event.actor()).to_string(),
            kind: kind.to_string(),
            causation_id: event.causation_id().map(|id| id.as_uuid().to_string()),
            created_at: event.created_at().to_string(),
            payload,
        }
    }
}

impl TryFrom<Event> for percept::Event {
    type Error = Error;

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        let payload = match event.kind.as_str() {
            "message.received" => {
                let body: MessageBody =
                    serde_json::from_value(event.payload).map_err(Error::BadPayload)?;
                Payload::MessageReceived {
                    content: body.content,
                }
            }
            other => return Err(Error::UnknownEventType(other.to_string())),
        };

        let id = EventId::from_uuid(parse_uuid(&event.id)?);
        let causation_id = match event.causation_id {
            Some(ref s) => Some(EventId::from_uuid(parse_uuid(s)?)),
            None => None,
        };
        let created_at = event
            .created_at
            .parse::<Timestamp>()
            .map_err(|_| Error::BadTimestamp(event.created_at.clone()))?;
        let actor = parse_actor(&event.actor)?;

        Ok(percept::Event::restore(
            id,
            event.seq,
            actor,
            causation_id,
            created_at,
            payload,
        ))
    }
}

fn actor_str(actor: Actor) -> &'static str {
    match actor {
        Actor::User => "user",
        Actor::Model => "model",
        Actor::System => "system",
    }
}

fn parse_actor(s: &str) -> Result<Actor, Error> {
    match s {
        "user" => Ok(Actor::User),
        "model" => Ok(Actor::Model),
        "system" => Ok(Actor::System),
        other => Err(Error::UnknownActor(other.to_string())),
    }
}

fn parse_uuid(s: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(s).map_err(|_| Error::BadUuid(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let cause = EventId::new();
        let original = percept::Event::restore(
            EventId::new(),
            7,
            Actor::Model,
            Some(cause),
            Timestamp::now(),
            Payload::MessageReceived {
                content: "hello world".to_string(),
            },
        );

        let json = serde_json::to_string(&Event::from(&original)).unwrap();
        let wire: Event = serde_json::from_str(&json).unwrap();
        let restored = percept::Event::try_from(wire).unwrap();

        assert!(restored.id() == original.id());
        assert_eq!(restored.seq(), original.seq());
        assert!(restored.actor() == original.actor());
        assert!(restored.causation_id() == original.causation_id());
        assert!(restored.created_at() == original.created_at());
        match restored.payload() {
            Payload::MessageReceived { content } => assert_eq!(content, "hello world"),
        }
    }

    #[test]
    fn unknown_type_deserializes_but_has_no_domain_form() {
        let json = r#"{
            "id": "0192d1f0-1111-7000-8000-000000000000",
            "seq": 1,
            "actor": "user",
            "type": "file.registered",
            "causation_id": null,
            "created_at": "2026-08-30T00:00:00Z",
            "payload": { "path": "/tmp/x" }
        }"#;

        let wire: Event = serde_json::from_str(json).expect("wire event deserializes");
        assert!(matches!(
            percept::Event::try_from(wire),
            Err(Error::UnknownEventType(_))
        ));
    }
}
