use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::percept::{Actor, Event, EventId, Payload};
use crate::shared::Timestamp;

/// An Event as it travels over the wire. Flat JSON:
/// `{ id, seq, actor, type, causation_id, created_at, payload }`.
/// `payload` shape depends on `type`.
#[derive(Serialize, Deserialize)]
pub struct EventDto {
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

impl From<&Event> for EventDto {
    fn from(event: &Event) -> Self {
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

impl TryFrom<EventDto> for Event {
    type Error = FromDtoError;

    fn try_from(dto: EventDto) -> Result<Self, Self::Error> {
        let payload = match dto.kind.as_str() {
            "message.received" => {
                let body: MessageBody =
                    serde_json::from_value(dto.payload).map_err(FromDtoError::BadPayload)?;
                Payload::MessageReceived {
                    content: body.content,
                }
            }
            other => return Err(FromDtoError::UnknownEventType(other.to_string())),
        };

        let id = EventId::from_uuid(parse_uuid(&dto.id)?);
        let causation_id = match dto.causation_id {
            Some(ref s) => Some(EventId::from_uuid(parse_uuid(s)?)),
            None => None,
        };
        let created_at = dto
            .created_at
            .parse::<Timestamp>()
            .map_err(|_| FromDtoError::BadTimestamp(dto.created_at.clone()))?;
        let actor = parse_actor(&dto.actor)?;

        Ok(Event::restore(id, dto.seq, actor, causation_id, created_at, payload))
    }
}

fn actor_str(actor: Actor) -> &'static str {
    match actor {
        Actor::User => "user",
        Actor::Model => "model",
        Actor::System => "system",
    }
}

fn parse_actor(s: &str) -> Result<Actor, FromDtoError> {
    match s {
        "user" => Ok(Actor::User),
        "model" => Ok(Actor::Model),
        "system" => Ok(Actor::System),
        other => Err(FromDtoError::UnknownActor(other.to_string())),
    }
}

fn parse_uuid(s: &str) -> Result<Uuid, FromDtoError> {
    Uuid::parse_str(s).map_err(|_| FromDtoError::BadUuid(s.to_string()))
}

/// Why an `EventDto` couldn't become a domain `Event`. An unknown `type`
/// or `actor` means the log was written by a newer build - the DTO still
/// deserializes, it just has no domain form here.
#[derive(Debug)]
pub enum FromDtoError {
    UnknownEventType(String),
    UnknownActor(String),
    BadUuid(String),
    BadTimestamp(String),
    BadPayload(serde_json::Error),
}

impl fmt::Display for FromDtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEventType(t) => write!(f, "unknown event type: {t}"),
            Self::UnknownActor(a) => write!(f, "unknown actor: {a}"),
            Self::BadUuid(s) => write!(f, "malformed uuid: {s}"),
            Self::BadTimestamp(s) => write!(f, "malformed timestamp: {s}"),
            Self::BadPayload(e) => write!(f, "malformed payload: {e}"),
        }
    }
}

impl std::error::Error for FromDtoError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let cause = EventId::new();
        let original = Event::restore(
            EventId::new(),
            7,
            Actor::Model,
            Some(cause),
            Timestamp::now(),
            Payload::MessageReceived {
                content: "hello world".to_string(),
            },
        );

        let json = serde_json::to_string(&EventDto::from(&original)).unwrap();
        let dto: EventDto = serde_json::from_str(&json).unwrap();
        let restored = Event::try_from(dto).unwrap();

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

        let dto: EventDto = serde_json::from_str(json).expect("dto deserializes");
        assert!(matches!(
            Event::try_from(dto),
            Err(FromDtoError::UnknownEventType(_))
        ));
    }
}
