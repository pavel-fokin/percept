use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::percept::{self, Actor, EventId, EventKind, NodeId, Payload, Usage};
use crate::shared::Timestamp;
use crate::store::Error;

/// `source` on the wire - the writer's name and its project root. No
/// migration for the string sources an older build wrote: a line
/// carrying one, or none at all, fails to load as a bad line.
#[derive(Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub path: PathBuf,
}

/// A `percept::Event` as it travels over the wire. Flat JSON:
/// `{ id, actor, source, type, causation_id, created_at, payload }`.
/// `payload` shape depends on `type`.
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub actor: String,
    pub source: Source,
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

#[derive(Serialize, Deserialize)]
struct NodeAddedBody {
    map: String,
    node: String,
    kind: String,
    name: String,
    properties: BTreeMap<String, String>,
    sources: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct NodeRemovedBody {
    map: String,
    node: String,
    reason: String,
    sources: Vec<String>,
}

/// The shared shape of `EdgeAdded` and `EdgeRemoved` - an edge carries
/// no id of its own, so `kind`, `from`, and `to` are all either needs.
#[derive(Serialize, Deserialize)]
struct EdgeBody {
    map: String,
    kind: String,
    from: String,
    to: String,
    sources: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ModelCalledBody {
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cached_tokens: Option<u64>,
}

const MESSAGE_RECEIVED: &str = "message.received";
const THOUGHT_RECORDED: &str = "thought.recorded";
const TOOL_CALLED: &str = "tool.called";
const TOOL_RESULTED: &str = "tool.resulted";
const NODE_ADDED: &str = "node.added";
const NODE_REMOVED: &str = "node.removed";
const EDGE_ADDED: &str = "edge.added";
const EDGE_REMOVED: &str = "edge.removed";
const MODEL_CALLED: &str = "model.called";

/// Every `type` the log records, for the error that lists them when a
/// caller names one that isn't here.
pub const KINDS: [&str; 9] = [
    MESSAGE_RECEIVED,
    THOUGHT_RECORDED,
    TOOL_CALLED,
    TOOL_RESULTED,
    NODE_ADDED,
    NODE_REMOVED,
    EDGE_ADDED,
    EDGE_REMOVED,
    MODEL_CALLED,
];

/// The wire `type` a kind serializes as.
fn kind(kind: EventKind) -> &'static str {
    match kind {
        EventKind::MessageReceived => MESSAGE_RECEIVED,
        EventKind::ThoughtRecorded => THOUGHT_RECORDED,
        EventKind::ToolCalled => TOOL_CALLED,
        EventKind::ToolResulted => TOOL_RESULTED,
        EventKind::NodeAdded => NODE_ADDED,
        EventKind::NodeRemoved => NODE_REMOVED,
        EventKind::EdgeAdded => EDGE_ADDED,
        EventKind::EdgeRemoved => EDGE_REMOVED,
        EventKind::ModelCalled => MODEL_CALLED,
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
        NODE_ADDED => Ok(EventKind::NodeAdded),
        NODE_REMOVED => Ok(EventKind::NodeRemoved),
        EDGE_ADDED => Ok(EventKind::EdgeAdded),
        EDGE_REMOVED => Ok(EventKind::EdgeRemoved),
        MODEL_CALLED => Ok(EventKind::ModelCalled),
        other => Err(Error::UnknownEventType(other.to_string())),
    }
}

/// One event as the JSONL line `percept.jsonl` stores.
pub fn encode(event: &percept::Event) -> String {
    serde_json::to_string(&Event::from(event)).expect("store::Event always serializes")
}

/// Longest string kept whole in a shortened payload, and the `content`
/// window when a caller names no other.
pub const PREVIEW_CHARS: usize = 120;

/// The payload key `Payload::content` travels under - one name for the
/// sites that cut or slice it on the wire.
const CONTENT: &str = "content";

/// What a summary line says about the `content` it cut, so a caller
/// can tell whether a second look is worth a call, and where to take it. Lives beside the
/// payload, not in it: the payload stays exactly what the log stores,
/// and these are facts about the output.
#[derive(Serialize)]
struct Preview {
    /// Length of the whole `content`, in characters.
    len: usize,
    /// Character offset of the search hit the window was cut around,
    /// so a caller can read on from there without guessing.
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    hit: Option<usize>,
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
/// a cut inside `arguments` is not. `hit` is a character range in
/// `content` the cut keeps in view - a search term's position - so a
/// line explains why it matched; without one the cut keeps the head.
/// `preview` is that window's size in characters; strings other than
/// `content` are cut at `PREVIEW_CHARS` whatever it is, since they are
/// the model's own short arguments, not the text a caller reads.
pub fn summarize(event: &percept::Event, hit: Option<Range<usize>>, preview: usize) -> String {
    let mut wire = Event::from(event);
    // `content` leaves the payload before `shorten` runs over the rest,
    // so it is cut once, here, at the caller's size.
    if let Some(fields) = wire.payload.as_object_mut() {
        fields.remove(CONTENT);
    }
    wire.payload = shorten(wire.payload);
    let preview = event.payload().content().and_then(|text| {
        let len = text.chars().count();
        let (shown, preview) = if len > preview {
            let cut = Preview {
                len,
                hit: hit.as_ref().map(|hit| hit.start),
            };
            let chars: Vec<char> = text.chars().collect();
            (window(&chars, hit.unwrap_or(0..0), preview), Some(cut))
        } else {
            (text.to_string(), None)
        };
        wire.payload[CONTENT] = Value::String(shown);
        preview
    });
    serde_json::to_string(&Summary {
        event: wire,
        preview,
    })
    .expect("store::Event always serializes")
}

/// `size` of `chars` with `keep` near the middle - or, when `keep` is
/// wider than the window, starting at it - pulled back to the ends of
/// the text rather than padded, and an ellipsis on each side that was
/// cut. Only called when `chars` is longer than the window, so some
/// side always is.
fn window(chars: &[char], keep: Range<usize>, size: usize) -> String {
    let slack = size.saturating_sub(keep.len());
    let start = keep.start.saturating_sub(slack / 2).min(chars.len() - size);
    let end = start + size;
    let mut out = String::with_capacity(end - start + 2);
    if start > 0 {
        out.push('\u{2026}');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('\u{2026}');
    }
    out
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
    let content = event
        .payload()
        .content()
        .ok_or_else(|| Error::NoRangeableContent(kind(event.kind()).to_string()))?;
    let mut wire = Event::from(event);

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

    wire.payload[CONTENT] = Value::String(chars[start..end].iter().collect());

    Ok(serde_json::to_string(&Summary {
        event: wire,
        preview: Some(Preview { len, hit: None }),
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
            let chars: Vec<char> = s.chars().collect();
            Value::String(if chars.len() > PREVIEW_CHARS {
                window(&chars, 0..0, PREVIEW_CHARS)
            } else {
                s
            })
        }
        Value::Array(items) => Value::Array(items.into_iter().map(shorten).collect()),
        Value::Object(fields) => {
            Value::Object(fields.into_iter().map(|(k, v)| (k, shorten(v))).collect())
        }
        other => other,
    }
}

/// `sources` on the wire - each `EventId` as its UUID string.
pub(super) fn ids(sources: &[EventId]) -> Vec<String> {
    sources.iter().map(|id| id.as_uuid().to_string()).collect()
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
            Payload::NodeAdded {
                map,
                node,
                kind,
                name,
                properties,
                sources,
            } => serde_json::to_value(NodeAddedBody {
                map: map.clone(),
                node: node.as_uuid().to_string(),
                kind: kind.clone(),
                name: name.clone(),
                properties: properties.clone(),
                sources: ids(sources),
            })
            .expect("NodeAddedBody always serializes"),
            Payload::NodeRemoved {
                map,
                node,
                reason,
                sources,
            } => serde_json::to_value(NodeRemovedBody {
                map: map.clone(),
                node: node.as_uuid().to_string(),
                reason: reason.clone(),
                sources: ids(sources),
            })
            .expect("NodeRemovedBody always serializes"),
            Payload::EdgeAdded {
                map,
                kind,
                from,
                to,
                sources,
            }
            | Payload::EdgeRemoved {
                map,
                kind,
                from,
                to,
                sources,
            } => serde_json::to_value(EdgeBody {
                map: map.clone(),
                kind: kind.clone(),
                from: from.as_uuid().to_string(),
                to: to.as_uuid().to_string(),
                sources: ids(sources),
            })
            .expect("EdgeBody always serializes"),
            Payload::ModelCalled(usage) => serde_json::to_value(ModelCalledBody {
                model: usage.model.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cached_tokens: usage.cached_tokens,
            })
            .expect("ModelCalledBody always serializes"),
        };

        Self {
            id: event.id().as_uuid().to_string(),
            actor: actor_name(event.actor()).to_string(),
            source: Source {
                name: event.source().name.clone(),
                path: event.source().path.clone(),
            },
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
            percept::Source {
                name: event.source.name,
                path: event.source.path,
            },
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
    source: percept::Source,
    kind: &str,
    causation_id: Option<EventId>,
    payload: Value,
) -> Result<percept::Event, Error> {
    let event = percept::Event::new(
        parse_actor(actor)?,
        source,
        causation_id,
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
        EventKind::NodeAdded => {
            let body: NodeAddedBody = serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::NodeAdded {
                map: body.map,
                node: parse_node_id(&body.node)?,
                kind: body.kind,
                name: body.name,
                properties: body.properties,
                sources: parse_event_ids(body.sources)?,
            })
        }
        EventKind::NodeRemoved => {
            let body: NodeRemovedBody =
                serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::NodeRemoved {
                map: body.map,
                node: parse_node_id(&body.node)?,
                reason: body.reason,
                sources: parse_event_ids(body.sources)?,
            })
        }
        kind @ (EventKind::EdgeAdded | EventKind::EdgeRemoved) => {
            let body: EdgeBody = serde_json::from_value(payload).map_err(Error::BadPayload)?;
            let (map, kind_name) = (body.map, body.kind);
            let from = parse_node_id(&body.from)?;
            let to = parse_node_id(&body.to)?;
            let sources = parse_event_ids(body.sources)?;
            Ok(if kind == EventKind::EdgeAdded {
                Payload::EdgeAdded {
                    map,
                    kind: kind_name,
                    from,
                    to,
                    sources,
                }
            } else {
                Payload::EdgeRemoved {
                    map,
                    kind: kind_name,
                    from,
                    to,
                    sources,
                }
            })
        }
        EventKind::ModelCalled => {
            let body: ModelCalledBody =
                serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::ModelCalled(Usage {
                model: body.model,
                input_tokens: body.input_tokens,
                output_tokens: body.output_tokens,
                cached_tokens: body.cached_tokens,
            }))
        }
    }
}

/// `sources` off the wire - each UUID string parsed to an `EventId`, so
/// one malformed entry fails the whole payload rather than being
/// dropped silently.
fn parse_event_ids(sources: Vec<String>) -> Result<Vec<EventId>, Error> {
    sources.iter().map(|s| parse_event_id(s)).collect()
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
fn parse_node_id(s: &str) -> Result<NodeId, Error> {
    Ok(NodeId::from_uuid(parse_uuid(s)?))
}

pub fn parse_event_id(s: &str) -> Result<EventId, Error> {
    Ok(EventId::from_uuid(parse_uuid(s)?))
}

fn parse_uuid(s: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(s).map_err(|_| Error::BadUuid(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{source, usage};

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
        let line: Value = serde_json::from_str(&summarize(&long, None, PREVIEW_CHARS)).unwrap();
        assert_eq!(line["preview"]["len"], 500);
        assert_eq!(
            line["payload"]["content"].as_str().unwrap().chars().count(),
            PREVIEW_CHARS + 1
        );

        let short = message(Actor::Model, "hi".to_string());
        let line: Value = serde_json::from_str(&summarize(&short, None, PREVIEW_CHARS)).unwrap();
        assert!(line.get("preview").is_none());
    }

    #[test]
    fn a_hit_deep_in_content_sits_inside_its_preview() {
        let text = format!("{}deploy{}", "a".repeat(400), "b".repeat(400));
        let event = message(Actor::Model, text);
        let line: Value =
            serde_json::from_str(&summarize(&event, Some(400..406), PREVIEW_CHARS)).unwrap();
        let cut = line["payload"]["content"].as_str().unwrap();
        assert!(cut.contains("deploy"));
        assert!(cut.starts_with('\u{2026}') && cut.ends_with('\u{2026}'));
        assert_eq!(cut.chars().count(), PREVIEW_CHARS + 2);
        assert_eq!(line["preview"]["match"], 400);
    }

    #[test]
    fn a_term_wider_than_half_the_window_still_fits_in_it() {
        let text = format!("{}deployment pipeline{}", "a".repeat(400), "b".repeat(400));
        let event = message(Actor::Model, text);
        let line: Value = serde_json::from_str(&summarize(&event, Some(400..419), 20)).unwrap();
        let cut = line["payload"]["content"].as_str().unwrap();
        assert!(cut.contains("deployment pipeline"), "{cut}");

        let line: Value = serde_json::from_str(&summarize(&event, Some(400..419), 10)).unwrap();
        let cut = line["payload"]["content"].as_str().unwrap();
        assert!(cut.contains("deployment"), "{cut}");
    }

    #[test]
    fn a_preview_without_a_hit_carries_no_match() {
        let event = message(Actor::Model, "x".repeat(500));
        let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
        assert!(line["preview"].get("match").is_none());
    }

    #[test]
    fn a_hit_near_the_end_pulls_the_window_back_rather_than_past_it() {
        let text = format!("{}deploy", "a".repeat(400));
        let event = message(Actor::Model, text);
        let line: Value =
            serde_json::from_str(&summarize(&event, Some(400..406), PREVIEW_CHARS)).unwrap();
        let cut = line["payload"]["content"].as_str().unwrap();
        assert!(cut.starts_with('\u{2026}') && cut.ends_with("deploy"));
        assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn the_preview_window_is_the_callers_size() {
        let event = message(Actor::Model, "x".repeat(500));
        let line: Value = serde_json::from_str(&summarize(&event, None, 10)).unwrap();
        assert_eq!(
            line["payload"]["content"].as_str().unwrap().chars().count(),
            11
        );

        let line: Value = serde_json::from_str(&summarize(&event, None, 1000)).unwrap();
        assert_eq!(
            line["payload"]["content"].as_str().unwrap().chars().count(),
            500
        );
        assert!(line.get("preview").is_none());
    }

    #[test]
    fn a_cut_inside_arguments_is_not_a_preview() {
        let call = percept::Event::restore(
            EventId::new(),
            Actor::Model,
            source("tui"),
            None,
            Timestamp::now(),
            Payload::ToolCalled {
                tool: "search_events".to_string(),
                arguments: format!(r#"{{"contains":["{}"]}}"#, "y".repeat(500)),
            },
        );
        let line: Value = serde_json::from_str(&summarize(&call, None, PREVIEW_CHARS)).unwrap();
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
            source("tui"),
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
        percept::Event::message_received(actor, content, source("tui"), None)
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
            source("tui"),
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
            source("tui"),
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
            source("tui"),
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
            source("tui"),
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
    fn model_called_round_trips_through_json() {
        let cause = EventId::new();
        let original = percept::Event::restore(
            EventId::new(),
            Actor::System,
            source("tui"),
            Some(cause),
            Timestamp::now(),
            Payload::ModelCalled(usage()),
        );

        let json = serde_json::to_string(&Event::from(&original)).unwrap();
        let wire: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.kind, "model.called");
        assert_eq!(wire.actor, "system");
        // Unreported cached tokens are left off the wire, not written
        // as null.
        assert!(wire.payload.get("cached_tokens").is_none());
        let restored = percept::Event::try_from(wire).unwrap();

        assert!(restored.actor() == Actor::System);
        assert!(restored.causation_id() == Some(cause));
        match restored.payload() {
            Payload::ModelCalled(restored) => assert_eq!(restored, &usage()),
            _ => panic!("expected ModelCalled"),
        }
    }

    #[test]
    fn node_added_round_trips_through_json() {
        let cited = EventId::new();
        let node = NodeId::new();
        let mut properties = BTreeMap::new();
        properties.insert(
            "summary".to_string(),
            "Same features on both stacks".to_string(),
        );
        let original = percept::Event::restore(
            EventId::new(),
            Actor::User,
            source("cli"),
            None,
            Timestamp::now(),
            Payload::NodeAdded {
                map: "decisions".to_string(),
                node,
                kind: "evidence".to_string(),
                name: "Both built in parallel".to_string(),
                properties: properties.clone(),
                sources: vec![cited],
            },
        );

        let json = serde_json::to_string(&Event::from(&original)).unwrap();
        let wire: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.kind, "node.added");
        let restored = percept::Event::try_from(wire).unwrap();

        match restored.payload() {
            Payload::NodeAdded {
                map,
                node: restored_node,
                kind,
                name,
                properties: restored_properties,
                sources,
            } => {
                assert_eq!(map, "decisions");
                assert!(*restored_node == node);
                assert_eq!(kind, "evidence");
                assert_eq!(name, "Both built in parallel");
                assert_eq!(*restored_properties, properties);
                assert!(sources == &vec![cited]);
            }
            _ => panic!("expected NodeAdded"),
        }
    }

    #[test]
    fn node_removed_round_trips_through_json() {
        let node = NodeId::new();
        let original = percept::Event::restore(
            EventId::new(),
            Actor::System,
            source("cli"),
            None,
            Timestamp::now(),
            Payload::NodeRemoved {
                map: "decisions".to_string(),
                node,
                reason: "superseded".to_string(),
                sources: Vec::new(),
            },
        );

        let json = serde_json::to_string(&Event::from(&original)).unwrap();
        let wire: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.kind, "node.removed");
        // Empty `sources` encodes as `[]`, never omitted.
        assert_eq!(wire.payload["sources"], serde_json::json!([]));
        let restored = percept::Event::try_from(wire).unwrap();

        match restored.payload() {
            Payload::NodeRemoved {
                map,
                node: restored_node,
                reason,
                sources,
            } => {
                assert_eq!(map, "decisions");
                assert!(*restored_node == node);
                assert_eq!(reason, "superseded");
                assert!(sources.is_empty());
            }
            _ => panic!("expected NodeRemoved"),
        }
    }

    #[test]
    fn edge_added_round_trips_through_json() {
        let from = NodeId::new();
        let to = NodeId::new();
        let original = percept::Event::restore(
            EventId::new(),
            Actor::System,
            source("cli"),
            None,
            Timestamp::now(),
            Payload::EdgeAdded {
                map: "decisions".to_string(),
                kind: "supports".to_string(),
                from,
                to,
                sources: Vec::new(),
            },
        );

        let json = serde_json::to_string(&Event::from(&original)).unwrap();
        let wire: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.kind, "edge.added");
        let restored = percept::Event::try_from(wire).unwrap();

        match restored.payload() {
            Payload::EdgeAdded {
                map,
                kind,
                from: restored_from,
                to: restored_to,
                ..
            } => {
                assert_eq!(map, "decisions");
                assert_eq!(kind, "supports");
                assert!(*restored_from == from);
                assert!(*restored_to == to);
            }
            _ => panic!("expected EdgeAdded"),
        }
    }

    #[test]
    fn a_malformed_source_in_a_node_added_payload_is_an_error() {
        let payload = serde_json::json!({
            "map": "decisions",
            "node": NodeId::new().as_uuid().to_string(),
            "kind": "evidence",
            "name": "x",
            "properties": {},
            "sources": ["not-a-uuid"],
        });

        let err = match decode("user", source("cli"), "node.added", None, payload) {
            Err(e) => e,
            Ok(_) => panic!("expected a malformed source to be rejected"),
        };
        assert!(matches!(err, Error::BadUuid(s) if s == "not-a-uuid"));
    }

    #[test]
    fn a_map_events_summary_carries_no_preview() {
        let event = percept::Event::restore(
            EventId::new(),
            Actor::User,
            source("cli"),
            None,
            Timestamp::now(),
            Payload::NodeAdded {
                map: "decisions".to_string(),
                node: NodeId::new(),
                kind: "evidence".to_string(),
                name: "Both built in parallel".to_string(),
                properties: BTreeMap::new(),
                sources: Vec::new(),
            },
        );

        let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
        assert!(line.get("preview").is_none());
        assert_eq!(line["payload"]["name"], "Both built in parallel");
    }

    #[test]
    fn a_model_called_summary_carries_no_preview() {
        let event = percept::Event::restore(
            EventId::new(),
            Actor::System,
            source("tui"),
            None,
            Timestamp::now(),
            Payload::ModelCalled(usage()),
        );

        let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
        assert!(line.get("preview").is_none());
        assert_eq!(line["payload"]["model"], "gpt-5");
        assert_eq!(line["payload"]["input_tokens"], 100);
    }

    #[test]
    fn unknown_type_deserializes_but_has_no_domain_form() {
        let json = r#"{
            "id": "0192d1f0-1111-7000-8000-000000000000",
            "seq": 1,
            "actor": "user",
            "source": {"name": "tui", "path": "/test"},
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
    fn a_null_source_fails_to_deserialize() {
        let json = r#"{
            "id": "0192d1f0-1111-7000-8000-000000000000",
            "actor": "user",
            "source": null,
            "type": "message.received",
            "created_at": "2026-08-30T00:00:00Z",
            "payload": { "content": "hi" }
        }"#;

        assert!(serde_json::from_str::<Event>(json).is_err());
    }

    #[test]
    fn a_bare_string_source_fails_to_deserialize() {
        let json = r#"{
            "id": "0192d1f0-1111-7000-8000-000000000000",
            "actor": "user",
            "source": "tui",
            "type": "message.received",
            "causation_id": null,
            "created_at": "2026-08-30T00:00:00Z",
            "payload": { "content": "hi" }
        }"#;

        assert!(serde_json::from_str::<Event>(json).is_err());
    }

    #[test]
    fn a_missing_source_fails_to_deserialize() {
        let json = r#"{
            "id": "0192d1f0-1111-7000-8000-000000000000",
            "seq": 1,
            "actor": "user",
            "type": "message.received",
            "causation_id": null,
            "created_at": "2026-08-30T00:00:00Z",
            "payload": { "content": "hi" }
        }"#;

        assert!(serde_json::from_str::<Event>(json).is_err());
    }
}
