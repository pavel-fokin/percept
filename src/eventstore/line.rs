use std::error::Error;

use jiff::Timestamp;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::{Actor, ClientSessionId, Event, EventId, Kind, Payload, Source};

/// One log line. The log format lives here, apart from the domain `Event`.
#[derive(Serialize, Deserialize)]
pub(super) struct EventLine {
    id: EventId,
    source: Source,
    actor: Actor,
    kind: String,
    payload: serde_json::Value,
    raw: Option<serde_json::Value>,
    created_at: Timestamp,
}

impl From<&Event> for EventLine {
    fn from(event: &Event) -> Self {
        let payload = match &event.payload {
            Payload::SessionCreated { session, client_session_id } => {
                json!({ "session": session, "client_session_id": client_session_id.as_str() })
            }
            Payload::SessionStarted { session } | Payload::SessionStopped { session } => {
                json!({ "session": session })
            }
            Payload::Message { session, content } => json!({ "session": session, "content": content }),
            Payload::ToolUsed { session, tool } => json!({ "session": session, "tool": tool }),
            Payload::MapCreated { map, kind } => json!({ "map": map, "kind": kind }),
            Payload::NodeAdded { map, node, kind, title, properties } => {
                json!({ "map": map, "node": node, "kind": kind, "title": title, "properties": properties })
            }
            Payload::EdgeAdded { map, kind, from, to } => {
                json!({ "map": map, "kind": kind, "from": from, "to": to })
            }
        };

        Self {
            id: event.id,
            source: event.source.clone(),
            actor: event.actor,
            kind: event.payload.kind().name().into(),
            payload,
            raw: event.raw.clone(),
            created_at: event.created_at,
        }
    }
}

impl TryFrom<EventLine> for Event {
    type Error = Box<dyn Error + Send + Sync>;

    fn try_from(line: EventLine) -> Result<Self, Self::Error> {
        let kind = Kind::from_name(&line.kind).ok_or_else(|| format!("unknown event kind {:?}", line.kind))?;

        let p = &line.payload;
        let payload = match kind {
            Kind::SessionCreated => Payload::SessionCreated {
                session: field(p, "session")?,
                client_session_id: ClientSessionId::new(field(p, "client_session_id")?),
            },
            Kind::SessionStarted => Payload::SessionStarted { session: field(p, "session")? },
            Kind::SessionStopped => Payload::SessionStopped { session: field(p, "session")? },
            Kind::Message => Payload::Message { session: field(p, "session")?, content: field(p, "content")? },
            Kind::ToolUsed => Payload::ToolUsed { session: field(p, "session")?, tool: field(p, "tool")? },
            Kind::MapCreated => Payload::MapCreated {
                map: field(p, "map")?,
                kind: field(p, "kind")?,
            },
            Kind::NodeAdded => Payload::NodeAdded {
                map: field(p, "map")?,
                node: field(p, "node")?,
                kind: field(p, "kind")?,
                title: field(p, "title")?,
                properties: if p.get("properties").is_some() { field(p, "properties")? } else { Default::default() },
            },
            Kind::EdgeAdded => Payload::EdgeAdded {
                map: field(p, "map")?,
                kind: field(p, "kind")?,
                from: field(p, "from")?,
                to: field(p, "to")?,
            },
        };

        Ok(Event {
            id: line.id,
            source: line.source,
            actor: line.actor,
            payload,
            raw: line.raw,
            created_at: line.created_at,
        })
    }
}

fn field<T: DeserializeOwned>(
    payload: &serde_json::Value,
    name: &str,
) -> Result<T, Box<dyn Error + Send + Sync>> {
    serde_json::from_value(payload[name].clone())
        .map_err(|error| format!("payload.{name}: {error}").into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::core::{MapId, NodeId, SessionId};

    #[test]
    fn every_payload_round_trips_through_a_line() {
        let session = SessionId::new();
        let raw = json!({"r": 1});
        let events = [
            Event::session_created(Source::fixture(), session, ClientSessionId::new("a".into())),
            Event::session_started(Source::fixture(), session, raw.clone()),
            Event::session_stopped(Source::fixture(), session, raw.clone()),
            Event::message(Source::fixture(), session, Actor::Human, "hi", raw.clone()),
            Event::tool_used(Source::fixture(), session, "Bash", raw),
            Event::map_created(Source::fixture(), Actor::Human, MapId::new(), "decisions"),
            Event::node_added(Source::fixture(), Actor::Agent, MapId::new(), NodeId::new(), "question", "Why?", BTreeMap::from([("definition".into(), "D".into())])),
            Event::edge_added(Source::fixture(), Actor::Agent, MapId::new(), "answers", NodeId::new(), NodeId::new()),
        ];

        for event in events {
            let text = serde_json::to_string(&EventLine::from(&event)).unwrap();
            let line: EventLine = serde_json::from_str(&text).unwrap();
            let back = Event::try_from(line).unwrap();

            assert_eq!(back.id, event.id);
            assert_eq!(back.source, event.source);
            assert_eq!(back.actor, event.actor);
            assert_eq!(back.payload, event.payload);
            assert_eq!(back.raw, event.raw);
            assert_eq!(back.created_at, event.created_at);
        }
    }

    #[test]
    fn a_node_line_without_properties_reads_as_empty() {
        let (map, node) = (MapId::new(), NodeId::new());
        let event = Event::node_added(Source::fixture(), Actor::Agent, map, node, "question", "Why?", BTreeMap::new());
        let mut line = EventLine::from(&event);
        line.payload = json!({ "map": map, "node": node, "kind": "question", "title": "Why?" });

        let back = Event::try_from(line).unwrap();

        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn a_legacy_map_line_with_a_title_still_reads() {
        let map = MapId::new();
        let event = Event::map_created(Source::fixture(), Actor::Human, map, "decisions");
        let mut line = EventLine::from(&event);
        line.payload = json!({ "map": map, "kind": "decisions", "title": "Event model" });

        let back = Event::try_from(line).unwrap();

        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn a_line_has_a_kind_and_a_payload_naming_the_session() {
        let session = SessionId::new();
        let event = Event::message(Source::fixture(), session, Actor::Human, "hi", json!({"r": 1}));

        let line = serde_json::to_value(EventLine::from(&event)).unwrap();

        assert_eq!(line["source"], json!({"client": "claude", "path": "/repo"}));
        assert_eq!(line["actor"], "human");
        assert_eq!(line["kind"], "message");
        assert_eq!(line["payload"], json!({"session": session, "content": "hi"}));
        assert_eq!(line["raw"], json!({"r": 1}));
    }

    #[test]
    fn unknown_kind_is_an_error() {
        let event = Event::message(Source::fixture(), SessionId::new(), Actor::Human, "hi", json!({}));
        let mut line = EventLine::from(&event);
        line.kind = "bogus".into();

        assert!(Event::try_from(line).is_err());
    }
}
