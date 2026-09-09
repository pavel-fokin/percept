use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::core::{Actor, EventId, EventKind, NodeId, Payload, Usage};
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

/// A `core::Event` as it travels over the wire. Flat JSON:
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
    /// Missing on a line written before short ids existed - `default`
    /// reads that as `0`, `Map::replay`'s sentinel for "not recorded".
    /// Omitted on encode at that same sentinel, so a payload a caller
    /// built with no `seq` at all still round-trips through `decode`.
    #[serde(default, skip_serializing_if = "is_zero")]
    seq: u32,
}

fn is_zero(seq: &u32) -> bool {
    *seq == 0
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

/// `lines` on the wire - `"40-58"`, or absent for the whole file.
#[derive(Serialize, Deserialize)]
struct FileCitedBody {
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lines: Option<String>,
    excerpt: String,
}

/// `Payload::FileCited.lines` as `"from-to"` - the wire spelling of a
/// 1-based inclusive range.
fn format_lines(lines: (u32, u32)) -> String {
    format!("{}-{}", lines.0, lines.1)
}

/// `"from-to"` back to `Payload::FileCited.lines` - shared with
/// `cli::publish`, which parses the same shape before it ever reaches
/// `decode`. `from` must be at least 1 and no greater than `to`: a
/// reversed or zero-based range names no lines a file has.
pub fn parse_lines(s: &str) -> Result<(u32, u32), Error> {
    let (from, to) = s
        .split_once('-')
        .ok_or_else(|| Error::BadLines(s.to_string()))?;
    let from: u32 = from.parse().map_err(|_| Error::BadLines(s.to_string()))?;
    let to: u32 = to.parse().map_err(|_| Error::BadLines(s.to_string()))?;
    if from == 0 || from > to {
        return Err(Error::BadLines(s.to_string()));
    }
    Ok((from, to))
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
const SESSION_STARTED: &str = "session.started";
const FILE_CITED: &str = "file.cited";

/// Every `type` the log records, for the error that lists them when a
/// caller names one that isn't here.
pub const KINDS: [&str; 11] = [
    MESSAGE_RECEIVED,
    THOUGHT_RECORDED,
    TOOL_CALLED,
    TOOL_RESULTED,
    NODE_ADDED,
    NODE_REMOVED,
    EDGE_ADDED,
    EDGE_REMOVED,
    MODEL_CALLED,
    SESSION_STARTED,
    FILE_CITED,
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
        EventKind::SessionStarted => SESSION_STARTED,
        EventKind::FileCited => FILE_CITED,
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
        SESSION_STARTED => Ok(EventKind::SessionStarted),
        FILE_CITED => Ok(EventKind::FileCited),
        other => Err(Error::UnknownEventType(other.to_string())),
    }
}

/// One event as the JSONL line `percept.jsonl` stores.
pub fn encode(event: &crate::core::Event) -> String {
    serde_json::to_string(&Event::from(event)).expect("store::Event always serializes")
}

pub use crate::core::PREVIEW_CHARS;

/// The payload key `Payload::content` travels under - one name for the
/// sites that cut or slice it on the wire.
const CONTENT: &str = "content";

/// The wire field `Payload::content` travels under for `event`'s kind -
/// `content` for every payload but `file.cited`, which calls its long
/// text `excerpt`. One lookup, so `summarize` and `excerpt` cut and
/// replace the same field they read.
fn content_key(kind: EventKind) -> &'static str {
    match kind {
        EventKind::FileCited => "excerpt",
        _ => CONTENT,
    }
}

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
pub fn summarize(event: &crate::core::Event, hit: Option<Range<usize>>, preview: usize) -> String {
    let mut wire = Event::from(event);
    let key = content_key(event.kind());
    // `content` (or `excerpt`) leaves the payload before `shorten` runs
    // over the rest, so it is cut once, here, at the caller's size.
    if let Some(fields) = wire.payload.as_object_mut() {
        fields.remove(key);
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
        wire.payload[key] = Value::String(shown);
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
    event: &crate::core::Event,
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

    wire.payload[content_key(event.kind())] = Value::String(chars[start..end].iter().collect());

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
pub fn ids(sources: &[EventId]) -> Vec<String> {
    sources.iter().map(|id| id.as_uuid().to_string()).collect()
}

impl From<&crate::core::Event> for Event {
    fn from(event: &crate::core::Event) -> Self {
        let payload = match event.payload() {
            // Both carry the same wire shape - one body, one arm.
            Payload::MessageReceived { content } | Payload::ThoughtRecorded { content } => {
                serde_json::to_value(MessageBody {
                    content: content.clone(),
                })
                .expect("MessageBody always serializes")
            }
            // `arguments` is JSON text when it came off the wire or
            // through `decode_payload`, but `App` commits what a model
            // streamed as it came, and a provider can hand over text
            // that is not one JSON value. That text is still what the
            // model said: it goes on the record as a JSON string, and
            // the tool's own parse tells the model what was wrong.
            Payload::ToolCalled { tool, arguments } => serde_json::to_value(ToolCalledBody {
                tool: tool.clone(),
                arguments: serde_json::from_str(arguments)
                    .unwrap_or_else(|_| Value::String(arguments.clone())),
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
                seq,
            } => serde_json::to_value(NodeAddedBody {
                map: map.clone(),
                node: node.as_uuid().to_string(),
                kind: kind.clone(),
                name: name.clone(),
                properties: properties.clone(),
                sources: ids(sources),
                seq: *seq,
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
            Payload::SessionStarted => Value::Object(serde_json::Map::new()),
            Payload::FileCited {
                path,
                lines,
                excerpt,
            } => serde_json::to_value(FileCitedBody {
                path: path.clone(),
                lines: lines.map(format_lines),
                excerpt: excerpt.clone(),
            })
            .expect("FileCitedBody always serializes"),
        };

        Self {
            id: event.id().as_uuid().to_string(),
            actor: event.actor().name().to_string(),
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

impl TryFrom<Event> for crate::core::Event {
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

        Ok(crate::core::Event::restore(
            id,
            actor,
            crate::core::Source {
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
    source: crate::core::Source,
    kind: &str,
    causation_id: Option<EventId>,
    payload: Value,
) -> Result<crate::core::Event, Error> {
    let event = crate::core::Event::new(
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
                seq: body.seq,
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
        EventKind::SessionStarted => Ok(Payload::SessionStarted),
        EventKind::FileCited => {
            let body: FileCitedBody = serde_json::from_value(payload).map_err(Error::BadPayload)?;
            Ok(Payload::FileCited {
                path: body.path,
                lines: body.lines.as_deref().map(parse_lines).transpose()?,
                excerpt: body.excerpt,
            })
        }
    }
}

/// `sources` off the wire - each UUID string parsed to an `EventId`, so
/// one malformed entry fails the whole payload rather than being
/// dropped silently.
fn parse_event_ids(sources: Vec<String>) -> Result<Vec<EventId>, Error> {
    sources.iter().map(|s| parse_event_id(s)).collect()
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
mod tests;
