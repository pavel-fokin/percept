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

/// `arguments` is a real nested object on the wire, so a caller can
/// index into it with `jq`. The domain holds it as text -
/// `From`/`TryFrom` parse and re-serialize across the seam.
#[derive(Serialize, Deserialize)]
struct ToolCalledBody {
    tool: String,
    arguments: Value,
}

#[derive(Serialize, Deserialize)]
struct ToolResultedBody {
    content: String,
}

const MESSAGE_RECEIVED: &str = "message.received";
const THOUGHT_RECORDED: &str = "thought.recorded";
const TOOL_CALLED: &str = "tool.called";
const TOOL_RESULTED: &str = "tool.resulted";

/// Every `type` the log records, for the error that lists them when a
/// caller names one that isn't here.
pub const KINDS: [&str; 4] = [
    MESSAGE_RECEIVED,
    THOUGHT_RECORDED,
    TOOL_CALLED,
    TOOL_RESULTED,
];

/// The wire `type` a kind serializes as.
fn kind(kind: EventKind) -> &'static str {
    match kind {
        EventKind::MessageReceived => MESSAGE_RECEIVED,
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

/// What a summary line says about the `content` it cut, so a caller
/// can tell whether a second look is worth a call. Lives beside the
/// payload, not in it: the payload stays exactly what the log stores,
/// and these are facts about the output.
#[derive(Serialize)]
struct Preview {
    /// Length of the whole `content`, in characters.
    len: usize,
}

/// A summary line: the wire event with `preview` on the envelope when
/// `content` was cut. Serialize-only - a stored line never carries
/// `preview`, so `Event` stays the one decode shape.
#[derive(Serialize)]
struct Summary {
    #[serde(flatten)]
    event: Event,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<Preview>,
}

/// One event as a cheap line: the same shape `encode` writes, with
/// every long string in the payload cut short. The payload stays a real
/// object, so one `jq` expression reads both this and `encode`'s
/// output. A caller reaches for `encode` deliberately, per the purpose
/// rule in AGENTS.md. `content` is the event's text - the one string
/// that runs long - so a cut to it is reported under `preview`, where
/// a cut inside `arguments` is not.
pub fn summarize(event: &percept::Event) -> String {
    let mut wire = Event::from(event);
    let preview = wire.payload["content"]
        .as_str()
        .map(|content| content.chars().count())
        .filter(|&len| len > PREVIEW_CHARS)
        .map(|len| Preview { len });
    wire.payload = shorten(wire.payload);
    serde_json::to_string(&Summary {
        event: wire,
        preview,
    })
    .expect("store::Event always serializes")
}

/// One event as a ranged slice: the same shape `encode` writes, with
/// `payload.content` cut to `[start, end)` characters and `preview.len`
/// naming the whole content's length - always, even when the slice
/// covers it all, so a caller never has to guess whether this line was
/// cut. `start` defaults to 0, `end` to the content's length and clamps
/// to it. Counts characters, never bytes, so a multi-byte character is
/// never split in half. An event whose payload carries no `content` -
/// `tool.called` - has nothing to slice.
pub fn excerpt(
    event: &percept::Event,
    start: Option<usize>,
    end: Option<usize>,
) -> Result<String, Error> {
    let mut wire = Event::from(event);
    let content = wire
        .payload
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::NoRangeableContent(wire.kind.clone()))?
        .to_string();

    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let start = start.unwrap_or(0);
    let end = end.unwrap_or(len).min(len);

    if start > len {
        return Err(Error::RangeStartPastEnd { start, len });
    }
    if start > end {
        return Err(Error::InvertedRange { start, end });
    }

    wire.payload["content"] = Value::String(chars[start..end].iter().collect());

    Ok(serde_json::to_string(&Summary {
        event: wire,
        preview: Some(Preview { len }),
    })
    .expect("store::Event always serializes"))
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
            // `arguments` was validated as JSON on the way in, either by
            // `decode_payload` parsing it or by `load` deserializing the
            // wire event - either way, re-parsing it here cannot fail.
            Payload::ToolCalled { tool, arguments } => serde_json::to_value(ToolCalledBody {
                tool: tool.clone(),
                arguments: serde_json::from_str(arguments)
                    .expect("ToolCalled arguments is validated JSON"),
            })
            .expect("ToolCalledBody always serializes"),
            Payload::ToolResulted { content } => serde_json::to_value(ToolResultedBody {
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
                tool: body.tool,
                // Kept as text - the domain routes by `tool` and never
                // parses `arguments`.
                arguments: serde_json::to_string(&body.arguments).expect("Value always serializes"),
            })
        }
        EventKind::ToolResulted => {
            let body: ToolResultedBody =
                serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::ToolResulted {
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
    fn a_summary_reports_the_length_of_a_cut_content_and_nothing_else() {
        let long = message(Actor::Model, "x".repeat(500));
        let line: Value = serde_json::from_str(&summarize(&long)).unwrap();
        assert_eq!(line["preview"]["len"], 500);
        assert_eq!(
            line["payload"]["content"].as_str().unwrap().chars().count(),
            PREVIEW_CHARS + 1
        );

        let short = message(Actor::Model, "hi".to_string());
        let line: Value = serde_json::from_str(&summarize(&short)).unwrap();
        assert!(line.get("preview").is_none());
    }

    #[test]
    fn a_cut_inside_arguments_is_not_a_preview() {
        let call = percept::Event::restore(
            EventId::new(),
            Actor::Model,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::ToolCalled {
                tool: "search_events".to_string(),
                arguments: format!(r#"{{"contains":["{}"]}}"#, "y".repeat(500)),
            },
        );
        let line: Value = serde_json::from_str(&summarize(&call)).unwrap();
        assert!(line.get("preview").is_none());
        assert!(line["payload"]["arguments"]["contains"][0]
            .as_str()
            .unwrap()
            .ends_with('\u{2026}'));
    }

    #[test]
    fn excerpt_slices_content_and_reports_the_whole_length() {
        let event = message(Actor::Model, "hello world".to_string());
        let line = excerpt(&event, Some(0), Some(5)).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["payload"]["content"], "hello");
        assert_eq!(value["preview"]["len"], 11);
    }

    #[test]
    fn excerpt_never_splits_a_multi_byte_character() {
        // "aaa" then two 3-byte euro signs - a byte slice at 4 would
        // split the first one.
        let event = message(Actor::Model, format!("aaa{}", "\u{20ac}\u{20ac}"));
        let line = excerpt(&event, Some(3), Some(4)).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["payload"]["content"], "\u{20ac}");
    }

    #[test]
    fn excerpt_defaults_start_to_zero_and_end_to_the_length() {
        let event = message(Actor::Model, "hi".to_string());
        let line = excerpt(&event, None, None).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["payload"]["content"], "hi");
        assert_eq!(value["preview"]["len"], 2);
    }

    #[test]
    fn excerpt_clamps_an_end_past_the_length() {
        let event = message(Actor::Model, "hi".to_string());
        let line = excerpt(&event, Some(0), Some(9000)).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["payload"]["content"], "hi");
    }

    #[test]
    fn excerpt_rejects_a_start_past_the_length_and_names_it() {
        let event = message(Actor::Model, "hi".to_string());
        let err = excerpt(&event, Some(9000), None).unwrap_err().to_string();
        assert_eq!(err, "start 9000 is past the end of content (2 characters)");
    }

    #[test]
    fn excerpt_rejects_an_inverted_range_and_names_both_ends() {
        let event = message(Actor::Model, "hello".to_string());
        let err = excerpt(&event, Some(4), Some(2)).unwrap_err().to_string();
        assert_eq!(err, "start 4 is not before end 2");
    }

    #[test]
    fn excerpt_on_a_tool_called_event_is_an_error() {
        let call = percept::Event::restore(
            EventId::new(),
            Actor::Model,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::ToolCalled {
                tool: "search_events".to_string(),
                arguments: "{}".to_string(),
            },
        );
        let err = excerpt(&call, None, None).unwrap_err().to_string();
        assert_eq!(err, "tool.called has no content to slice");
    }

    fn message(actor: Actor, content: String) -> percept::Event {
        percept::Event::restore(
            EventId::new(),
            actor,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::MessageReceived { content },
        )
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
                tool: "search_events".to_string(),
                arguments: r#"{"sources":["tui"],"size":5}"#.to_string(),
            },
        );

        let wire = Event::from(&original);
        assert_eq!(wire.kind, "tool.called");
        // `arguments` is a real object on the wire, indexable by jq.
        assert_eq!(wire.payload["arguments"]["size"], 5);

        let json = serde_json::to_string(&wire).unwrap();
        let reparsed: Event = serde_json::from_str(&json).unwrap();
        let restored = percept::Event::try_from(reparsed).unwrap();

        match restored.payload() {
            Payload::ToolCalled { tool, arguments } => {
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
            Payload::ToolResulted { content } => assert_eq!(content, "3 events"),
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
