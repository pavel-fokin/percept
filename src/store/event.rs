use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::percept::{self, Actor, EventId, Payload};
use crate::shared::Timestamp;
use crate::store::Error;

/// A `percept::Event` as it travels over the wire. Flat JSON:
/// `{ id, actor, source, type, causation_id, created_at, payload }`.
/// `payload` shape depends on `type`.
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub actor: String,
    /// Absent, null, or blank - a line written before the field
    /// existed, or by a writer that leaves it unset - loads as
    /// `"unknown"`.
    pub source: Option<String>,
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
            actor: actor_str(event.actor()).to_string(),
            source: Some(event.source().to_string()),
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
        let payload = decode_payload(&event.kind, event.payload)?;

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
            actor,
            named_source(event.source),
            causation_id,
            created_at,
            payload,
        ))
    }
}

/// Builds a fresh domain event from the parts a writer supplies - the
/// inbound half of the serde boundary. `kind` and `payload` are checked
/// against the same shapes `load` accepts, so one place decides what a
/// payload of each type may hold.
pub fn decode(
    actor: &str,
    source: String,
    kind: &str,
    payload: Value,
) -> Result<percept::Event, Error> {
    let event = percept::Event::new(
        parse_actor(actor)?,
        source,
        None,
        decode_payload(kind, payload.clone())?,
    );

    // `load` drops unknown payload fields on purpose, so a log written
    // by an older build still reads. Inbound, that same tolerance would
    // record less than the caller passed and report success.
    if Event::from(&event).payload != payload {
        return Err(Error::UnrecordedPayloadFields(kind.to_string()));
    }
    Ok(event)
}

fn decode_payload(kind: &str, payload: Value) -> Result<Payload, Error> {
    match kind {
        "message.received" => {
            let body: MessageBody = serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::MessageReceived {
                content: body.content,
            })
        }
        other => Err(Error::UnknownEventType(other.to_string())),
    }
}

/// Every event names a writer. A line that leaves `source` absent,
/// null, or blank names nobody, so it reads as `unknown` rather than
/// as a writer whose name happens to be empty.
fn named_source(source: Option<String>) -> String {
    source
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string())
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
            Actor::Model,
            "tui".to_string(),
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
        assert_eq!(restored.source(), original.source());
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

    #[test]
    fn explicit_null_source_and_absent_causation_id_load() {
        let json = r#"{
            "id": "0192d1f0-1111-7000-8000-000000000000",
            "actor": "user",
            "source": null,
            "type": "message.received",
            "created_at": "2026-08-30T00:00:00Z",
            "payload": { "content": "hi" }
        }"#;

        let wire: Event = serde_json::from_str(json).expect("wire event deserializes");
        let event = percept::Event::try_from(wire).expect("known event type restores");
        assert_eq!(event.source(), "unknown");
        assert!(event.causation_id().is_none());
    }

    #[test]
    fn legacy_line_with_seq_and_no_source_loads_as_unknown_source() {
        let json = r#"{
            "id": "0192d1f0-1111-7000-8000-000000000000",
            "seq": 1,
            "actor": "user",
            "type": "message.received",
            "causation_id": null,
            "created_at": "2026-08-30T00:00:00Z",
            "payload": { "content": "hi" }
        }"#;

        let wire: Event = serde_json::from_str(json).expect("wire event deserializes");
        let event = percept::Event::try_from(wire).expect("known event type restores");
        assert_eq!(event.source(), "unknown");
    }
}
