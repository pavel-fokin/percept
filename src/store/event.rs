use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::percept::{self, Actor, EventId, EventKind, Payload};
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

/// `arguments` is a real nested object on the wire, like `ToolUsed`'s
/// body, so a caller can index into it with `jq`. The domain holds it
/// as text - `From`/`TryFrom` parse and re-serialize across the seam.
#[derive(Serialize, Deserialize)]
struct ToolCalledBody {
    call_id: String,
    tool: String,
    arguments: Value,
}

#[derive(Serialize, Deserialize)]
struct ToolResultedBody {
    call_id: String,
    content: String,
}

const MESSAGE_RECEIVED: &str = "message.received";
const TOOL_USED: &str = "tool.used";
const THOUGHT_RECORDED: &str = "thought.recorded";
const TOOL_CALLED: &str = "tool.called";
const TOOL_RESULTED: &str = "tool.resulted";

/// Every `type` the log records, for the error that lists them when a
/// caller names one that isn't here.
pub const KINDS: [&str; 5] = [
    MESSAGE_RECEIVED,
    TOOL_USED,
    THOUGHT_RECORDED,
    TOOL_CALLED,
    TOOL_RESULTED,
];

/// The wire `type` a kind serializes as.
fn kind(kind: EventKind) -> &'static str {
    match kind {
        EventKind::MessageReceived => MESSAGE_RECEIVED,
        EventKind::ToolUsed => TOOL_USED,
        EventKind::ThoughtRecorded => THOUGHT_RECORDED,
        EventKind::ToolCalled => TOOL_CALLED,
        EventKind::ToolResulted => TOOL_RESULTED,
    }
}

/// An `EventKind` from its wire spelling - so a caller filtering by
/// type parses once rather than comparing every event as text.
pub fn parse_kind(s: &str) -> Result<EventKind, Error> {
    match s {
        MESSAGE_RECEIVED => Ok(EventKind::MessageReceived),
        TOOL_USED => Ok(EventKind::ToolUsed),
        THOUGHT_RECORDED => Ok(EventKind::ThoughtRecorded),
        TOOL_CALLED => Ok(EventKind::ToolCalled),
        TOOL_RESULTED => Ok(EventKind::ToolResulted),
        other => Err(Error::UnknownEventType(other.to_string())),
    }
}

/// One event as the JSONL line `percept.jsonl` stores.
pub fn encode(event: &percept::Event) -> String {
    serde_json::to_string(&Event::from(event)).expect("store::Event always serializes")
}

/// Longest string kept whole in a shortened payload.
const PREVIEW_CHARS: usize = 120;

/// One event as a cheap line: the same shape `encode` writes, with
/// every long string in the payload cut short. The payload stays a real
/// object, so one `jq` expression reads both this and `encode`'s
/// output. A caller reaches for `encode` deliberately, per the purpose
/// rule in AGENTS.md.
pub fn summarize(event: &percept::Event) -> String {
    let mut wire = Event::from(event);
    wire.payload = shorten(wire.payload);
    serde_json::to_string(&wire).expect("store::Event always serializes")
}

/// Cuts every long string in `payload`, keeping its structure. Cutting
/// the serialized text instead would leave a fragment no JSON tool
/// could read. Counts characters, never bytes, so a multi-byte
/// character is never split in half.
fn shorten(payload: Value) -> Value {
    match payload {
        Value::String(s) => {
            let mut chars = s.chars();
            let kept: String = chars.by_ref().take(PREVIEW_CHARS).collect();
            Value::String(if chars.next().is_some() {
                format!("{kept}\u{2026}")
            } else {
                kept
            })
        }
        Value::Array(items) => Value::Array(items.into_iter().map(shorten).collect()),
        Value::Object(fields) => {
            Value::Object(fields.into_iter().map(|(k, v)| (k, shorten(v))).collect())
        }
        other => other,
    }
}

impl From<&percept::Event> for Event {
    fn from(event: &percept::Event) -> Self {
        let payload = match event.payload() {
            // Both carry the same wire shape - one body, one arm.
            Payload::MessageReceived { content } | Payload::ThoughtRecorded { content } => {
                serde_json::to_value(MessageBody {
                    content: content.clone(),
                })
                .expect("MessageBody always serializes")
            }
            // `body` was validated as JSON on the way in, either by
            // `decode_payload` parsing it or by `load` deserializing the
            // wire event - either way, re-parsing it here cannot fail.
            Payload::ToolUsed { body } => {
                serde_json::from_str(body).expect("ToolUsed body is validated JSON")
            }
            // `arguments` was validated as JSON the same way `body` is.
            Payload::ToolCalled {
                call_id,
                tool,
                arguments,
            } => serde_json::to_value(ToolCalledBody {
                call_id: call_id.clone(),
                tool: tool.clone(),
                arguments: serde_json::from_str(arguments)
                    .expect("ToolCalled arguments is validated JSON"),
            })
            .expect("ToolCalledBody always serializes"),
            Payload::ToolResulted { call_id, content } => serde_json::to_value(ToolResultedBody {
                call_id: call_id.clone(),
                content: content.clone(),
            })
            .expect("ToolResultedBody always serializes"),
        };

        Self {
            id: event.id().as_uuid().to_string(),
            actor: actor_name(event.actor()).to_string(),
            source: Some(event.source().to_string()),
            kind: kind(event.kind()).to_string(),
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
    match parse_kind(kind)? {
        EventKind::MessageReceived => {
            let body: MessageBody = serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::MessageReceived {
                content: body.content,
            })
        }
        // Opaque: the domain never reads a tool call, so `body` keeps
        // the canonical serialization of whatever object the source
        // sent, unparsed.
        EventKind::ToolUsed => Ok(Payload::ToolUsed {
            body: serde_json::to_string(&payload).expect("Value always serializes"),
        }),
        EventKind::ThoughtRecorded => {
            let body: MessageBody = serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::ThoughtRecorded {
                content: body.content,
            })
        }
        EventKind::ToolCalled => {
            let body: ToolCalledBody =
                serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::ToolCalled {
                call_id: body.call_id,
                tool: body.tool,
                // Kept as text, like `ToolUsed`'s body - the domain
                // routes by `tool` and never parses `arguments`.
                arguments: serde_json::to_string(&body.arguments).expect("Value always serializes"),
            })
        }
        EventKind::ToolResulted => {
            let body: ToolResultedBody =
                serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::ToolResulted {
                call_id: body.call_id,
                content: body.content,
            })
        }
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

fn actor_name(actor: Actor) -> &'static str {
    match actor {
        Actor::User => "user",
        Actor::Model => "model",
        Actor::System => "system",
    }
}

pub fn parse_actor(s: &str) -> Result<Actor, Error> {
    match s {
        "user" => Ok(Actor::User),
        "model" => Ok(Actor::Model),
        "system" => Ok(Actor::System),
        other => Err(Error::UnknownActor(other.to_string())),
    }
}

/// An `EventId` from its wire spelling - so a caller comparing ids
/// parses once rather than rendering every event to compare as text.
pub fn parse_event_id(s: &str) -> Result<EventId, Error> {
    Ok(EventId::from_uuid(parse_uuid(s)?))
}

fn parse_uuid(s: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(s).map_err(|_| Error::BadUuid(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_payload_is_left_alone() {
        let payload = serde_json::json!({"content": "hi"});
        assert_eq!(shorten(payload.clone()), payload);
    }

    #[test]
    fn a_long_string_is_cut_but_its_object_keeps_its_shape() {
        let payload = serde_json::json!({
            "tool_name": "Edit",
            "tool_result": "a".repeat(500),
        });
        let short = shorten(payload);

        // Still an object, so one jq expression reads this and the
        // whole payload alike.
        assert_eq!(short["tool_name"], "Edit");
        let cut = short["tool_result"].as_str().unwrap();
        assert!(cut.ends_with('\u{2026}'));
        assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn a_cut_never_splits_a_multi_byte_character() {
        // 119 ascii chars, then a 3-byte character straddling the cut.
        // Truncating by bytes would split it.
        let payload = serde_json::json!({ "c": format!("{}\u{20ac}\u{20ac}", "a".repeat(119)) });
        let short = shorten(payload);

        let cut = short["c"].as_str().unwrap();
        assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
        assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn nested_and_array_values_are_reached() {
        let payload = serde_json::json!({"a": {"b": ["x".repeat(500)]}});
        let short = shorten(payload);

        assert_eq!(
            short["a"]["b"][0].as_str().unwrap().chars().count(),
            PREVIEW_CHARS + 1
        );
    }

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
            _ => panic!("expected MessageReceived"),
        }
    }

    #[test]
    fn tool_used_round_trips_through_json_as_a_nested_object() {
        let original = percept::Event::restore(
            EventId::new(),
            Actor::Model,
            "claude-code".to_string(),
            None,
            Timestamp::now(),
            Payload::ToolUsed {
                body: r#"{"tool_name":"Edit","tool_input":{"file_path":"/x/y.rs"}}"#.to_string(),
            },
        );

        let wire = Event::from(&original);
        // The wire payload is a real nested object, not an escaped
        // string, so a caller can index into it with a JSON tool.
        assert_eq!(wire.payload["tool_input"]["file_path"], "/x/y.rs");

        let json = serde_json::to_string(&wire).unwrap();
        let reparsed: Event = serde_json::from_str(&json).unwrap();
        let restored = percept::Event::try_from(reparsed).unwrap();

        match restored.payload() {
            Payload::ToolUsed { body } => {
                let value: Value = serde_json::from_str(body).unwrap();
                assert_eq!(value["tool_input"]["file_path"], "/x/y.rs");
            }
            _ => panic!("expected ToolUsed"),
        }
    }

    #[test]
    fn thought_recorded_round_trips_through_json() {
        let original = percept::Event::restore(
            EventId::new(),
            Actor::Model,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::ThoughtRecorded {
                content: "let me think".to_string(),
            },
        );

        let json = serde_json::to_string(&Event::from(&original)).unwrap();
        let wire: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.kind, "thought.recorded");
        let restored = percept::Event::try_from(wire).unwrap();

        match restored.payload() {
            Payload::ThoughtRecorded { content } => assert_eq!(content, "let me think"),
            _ => panic!("expected ThoughtRecorded"),
        }
    }

    #[test]
    fn tool_called_round_trips_with_arguments_as_a_nested_object() {
        let original = percept::Event::restore(
            EventId::new(),
            Actor::Model,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::ToolCalled {
                call_id: "c1".to_string(),
                tool: "search_events".to_string(),
                arguments: r#"{"sources":["tui"],"size":5}"#.to_string(),
            },
        );

        let wire = Event::from(&original);
        assert_eq!(wire.kind, "tool.called");
        // `arguments` is a real object on the wire, indexable by jq.
        assert_eq!(wire.payload["arguments"]["size"], 5);
        assert_eq!(wire.payload["call_id"], "c1");

        let json = serde_json::to_string(&wire).unwrap();
        let reparsed: Event = serde_json::from_str(&json).unwrap();
        let restored = percept::Event::try_from(reparsed).unwrap();

        match restored.payload() {
            Payload::ToolCalled {
                call_id,
                tool,
                arguments,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(tool, "search_events");
                let value: Value = serde_json::from_str(arguments).unwrap();
                assert_eq!(value["size"], 5);
            }
            _ => panic!("expected ToolCalled"),
        }
    }

    #[test]
    fn tool_resulted_round_trips_through_json() {
        let cause = EventId::new();
        let original = percept::Event::restore(
            EventId::new(),
            Actor::System,
            "tui".to_string(),
            Some(cause),
            Timestamp::now(),
            Payload::ToolResulted {
                call_id: "c1".to_string(),
                content: "3 events".to_string(),
            },
        );

        let json = serde_json::to_string(&Event::from(&original)).unwrap();
        let wire: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.kind, "tool.resulted");
        assert_eq!(wire.actor, "system");
        let restored = percept::Event::try_from(wire).unwrap();

        assert!(restored.actor() == Actor::System);
        assert!(restored.causation_id() == Some(cause));
        match restored.payload() {
            Payload::ToolResulted { call_id, content } => {
                assert_eq!(call_id, "c1");
                assert_eq!(content, "3 events");
            }
            _ => panic!("expected ToolResulted"),
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
