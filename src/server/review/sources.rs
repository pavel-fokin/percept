//! What a row's `sources` resolve to: the events a node's `sources`
//! name, as `GET /api/review` sends them - a message with the proposal
//! a human's "yes" answered, a file's path and excerpt, anything else
//! by its type, or `missing` when the log no longer holds it.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::core::{Actor, Event, EventId, Payload};

/// A source entry's `content` or `excerpt` is sent whole up to this
/// many characters; beyond it the entry is cut at a character boundary
/// and marked `truncated`. The log is large and a row's exchange can
/// quote a long reply or a long file - this keeps one entry from
/// blowing up a request that lists many rows.
const MAX_SOURCE_CHARS: usize = 4000;

/// `text` cut to `MAX_SOURCE_CHARS`, and whether it was.
fn truncate(text: &str) -> (String, bool) {
    if text.chars().count() <= MAX_SOURCE_CHARS {
        (text.to_string(), false)
    } else {
        (text.chars().take(MAX_SOURCE_CHARS).collect(), true)
    }
}

/// Every event loaded for one request, indexed by id - built once so
/// resolving a node's `sources` never calls `log.get` per source; the
/// log is large and a request lists many rows.
pub struct EventIndex<'a> {
    events: &'a [Event],
    by_id: HashMap<EventId, &'a Event>,
}

impl<'a> EventIndex<'a> {
    pub fn new(events: &'a [Event]) -> Self {
        Self {
            events,
            by_id: events.iter().map(|event| (event.id(), event)).collect(),
        }
    }

    /// The latest `message.received` from `Actor::Agent`, in the same
    /// source as `message` (same name and path) and `created_at`
    /// before it - the proposal a human's "yes" answered, or `None`
    /// when there is none.
    fn proposal_of(&self, message: &Event) -> Option<Value> {
        self.events
            .iter()
            .filter(|event| matches!(event.actor(), Actor::Agent))
            .filter(|event| matches!(event.payload(), Payload::MessageReceived { .. }))
            .filter(|event| event.source() == message.source())
            .filter(|event| event.created_at() < message.created_at())
            .max_by_key(|event| event.created_at())
            .map(|event| {
                let content = match event.payload() {
                    Payload::MessageReceived { content } => content,
                    _ => unreachable!("filtered to message.received above"),
                };
                let (content, _) = truncate(content);
                json!({
                    "id": event.id().as_uuid().to_string(),
                    "at": event.created_at().to_string(),
                    "content": content,
                })
            })
    }

    /// One entry of a node's `sources`: the event `id` names, read as
    /// what a source line documents - a message, a file, anything
    /// else, or `missing` when the log no longer holds it.
    fn entry(&self, id: EventId) -> Value {
        let Some(event) = self.by_id.get(&id) else {
            return json!({ "kind": "missing", "id": id.as_uuid().to_string() });
        };
        let base = json!({
            "id": event.id().as_uuid().to_string(),
            "at": event.created_at().to_string(),
        });
        match event.payload() {
            Payload::MessageReceived { content } => {
                let (content, truncated) = truncate(content);
                let mut entry = base;
                entry["kind"] = json!("message");
                entry["actor"] = json!(match event.actor() {
                    Actor::Agent => "agent",
                    _ => "human",
                });
                entry["client"] = json!(event.source().name);
                entry["content"] = json!(content);
                entry["proposal"] = match event.actor() {
                    Actor::Agent => Value::Null,
                    _ => self.proposal_of(event).unwrap_or(Value::Null),
                };
                entry["truncated"] = json!(truncated);
                entry
            }
            Payload::FileCited { path, lines, excerpt } => {
                let (excerpt, truncated) = truncate(excerpt);
                let mut entry = base;
                entry["kind"] = json!("file");
                entry["path"] = json!(path.to_string_lossy());
                entry["lines"] = match lines {
                    Some((from, to)) => json!([from, to]),
                    None => Value::Null,
                };
                entry["excerpt"] = json!(excerpt);
                entry["truncated"] = json!(truncated);
                entry
            }
            _ => {
                let mut entry = base;
                entry["kind"] = json!("event");
                entry["type"] = json!(crate::store::Event::from(*event).kind);
                entry
            }
        }
    }

    /// A node's `sources` as JSON, one entry per id in order.
    pub fn sources_json(&self, sources: &[EventId]) -> Vec<Value> {
        sources.iter().map(|id| self.entry(*id)).collect()
    }
}
