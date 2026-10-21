use std::path::PathBuf;

use jiff::Timestamp;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::{Actor, ClientAgentId, ClientSessionId, Event, EventId, Kind, Name, Payload, Reference, Source};
use crate::shared::{Error, Result};

/// One log line. The log format lives here, apart from the domain `Event`.
#[derive(Serialize, Deserialize)]
pub(super) struct EventLine {
    id: EventId,
    source: SourceLine,
    actor: Actor,
    kind: String,
    payload: serde_json::Value,
    raw: Option<serde_json::Value>,
    created_at: Timestamp,
}

impl From<&Event> for EventLine {
    fn from(event: &Event) -> Self {
        let payload = match &event.payload {
            Payload::SessionCreated { session, project, client_session_id } => {
                json!({ "session": session, "project": project, "client_session_id": client_session_id.as_str() })
            }
            Payload::SessionStarted { session } | Payload::SessionStopped { session } => {
                json!({ "session": session })
            }
            Payload::Message { session, content } => json!({ "session": session, "content": content }),
            Payload::ToolUsed { session, tool, input, output } => {
                json!({ "session": session, "tool": tool, "input": input, "output": output })
            }
            Payload::SubagentStarted { session, subagent, agent, task } => {
                json!({ "session": session, "subagent": subagent.as_str(), "agent": agent, "task": task })
            }
            Payload::SubagentReported { session, subagent, content } => {
                json!({ "session": session, "subagent": subagent.as_str(), "content": content })
            }
            Payload::ProjectCreated { project, name, root } => json!({ "project": project, "name": name.as_str(), "root": root }),
            Payload::BoardCreated { board, project, name } => json!({ "board": board, "project": project, "name": name.as_str() }),
            Payload::MapCreated { map, project, kind, definition } => json!({ "map": map, "project": project, "kind": kind, "definition": definition }),
            Payload::NodeAdded { map, node, kind, title, properties, references } => {
                let references: Vec<ReferenceLine> = references.iter().map(ReferenceLine::from).collect();

                json!({ "map": map, "node": node, "kind": kind, "title": title, "properties": properties, "references": references })
            }
            Payload::NodeChanged { map, node, title, properties, references } => {
                let references: Vec<ReferenceLine> = references.iter().map(ReferenceLine::from).collect();

                json!({ "map": map, "node": node, "title": title, "properties": properties, "references": references })
            }
            Payload::NodeRemoved { map, node } => json!({ "map": map, "node": node }),
            Payload::EdgeAdded { map, kind, from, to } | Payload::EdgeRemoved { map, kind, from, to } => {
                json!({ "map": map, "kind": kind, "from": from, "to": to })
            }
        };

        Self {
            id: event.id,
            source: SourceLine::from(&event.source),
            actor: event.actor,
            kind: event.payload.kind().name().into(),
            payload,
            raw: event.raw.clone(),
            created_at: event.created_at,
        }
    }
}

impl TryFrom<EventLine> for Event {
    type Error = Error;

    fn try_from(line: EventLine) -> Result<Self> {
        let kind = Kind::from_name(&line.kind).ok_or_else(|| format!("unknown event kind {:?}", line.kind))?;

        let p = &line.payload;
        let payload = match kind {
            Kind::SessionCreated => Payload::SessionCreated {
                session: field(p, "session")?,
                project: field(p, "project")?,
                client_session_id: ClientSessionId::new(field(p, "client_session_id")?),
            },
            Kind::SessionStarted => Payload::SessionStarted { session: field(p, "session")? },
            Kind::SessionStopped => Payload::SessionStopped { session: field(p, "session")? },
            Kind::Message => Payload::Message { session: field(p, "session")?, content: field(p, "content")? },
            Kind::ToolUsed => Payload::ToolUsed {
                session: field(p, "session")?,
                tool: field(p, "tool")?,
                input: field(p, "input")?,
                output: field(p, "output")?,
            },
            Kind::SubagentStarted => Payload::SubagentStarted {
                session: field(p, "session")?,
                subagent: ClientAgentId::new(field(p, "subagent")?),
                agent: field(p, "agent")?,
                task: field(p, "task")?,
            },
            Kind::SubagentReported => Payload::SubagentReported {
                session: field(p, "session")?,
                subagent: ClientAgentId::new(field(p, "subagent")?),
                content: field(p, "content")?,
            },
            Kind::ProjectCreated => Payload::ProjectCreated {
                project: field(p, "project")?,
                name: Name::new(&field::<String>(p, "name")?)?,
                root: field(p, "root")?,
            },
            Kind::BoardCreated => Payload::BoardCreated {
                board: field(p, "board")?,
                project: field(p, "project")?,
                name: Name::new(&field::<String>(p, "name")?)?,
            },
            Kind::MapCreated => Payload::MapCreated {
                map: field(p, "map")?,
                project: field(p, "project")?,
                kind: field(p, "kind")?,
                definition: field(p, "definition")?,
            },
            Kind::NodeAdded => Payload::NodeAdded {
                map: field(p, "map")?,
                node: field(p, "node")?,
                kind: field(p, "kind")?,
                title: field(p, "title")?,
                properties: if p.get("properties").is_some() { field(p, "properties")? } else { Default::default() },
                references: if p.get("references").is_some() {
                    field::<Vec<ReferenceLine>>(p, "references")?.into_iter().map(Reference::from).collect()
                } else {
                    Vec::new()
                },
            },
            Kind::NodeChanged => Payload::NodeChanged {
                map: field(p, "map")?,
                node: field(p, "node")?,
                title: field(p, "title")?,
                properties: field(p, "properties")?,
                references: field::<Vec<ReferenceLine>>(p, "references")?.into_iter().map(Reference::from).collect(),
            },
            Kind::NodeRemoved => Payload::NodeRemoved { map: field(p, "map")?, node: field(p, "node")? },
            Kind::EdgeAdded => Payload::EdgeAdded {
                map: field(p, "map")?,
                kind: field(p, "kind")?,
                from: field(p, "from")?,
                to: field(p, "to")?,
            },
            Kind::EdgeRemoved => Payload::EdgeRemoved {
                map: field(p, "map")?,
                kind: field(p, "kind")?,
                from: field(p, "from")?,
                to: field(p, "to")?,
            },
        };

        Ok(Event {
            id: line.id,
            source: line.source.into(),
            actor: line.actor,
            payload,
            raw: line.raw,
            created_at: line.created_at,
        })
    }
}

/// A source with no `path` is the web.
#[derive(Serialize, Deserialize)]
struct SourceLine {
    client: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<PathBuf>,
}

impl From<&Source> for SourceLine {
    fn from(source: &Source) -> Self {
        match source {
            Source::Checkout { client, path } => Self { client: client.clone(), path: Some(path.clone()) },
            Source::Web => Self { client: Source::WEB_CLIENT.into(), path: None },
        }
    }
}

impl From<SourceLine> for Source {
    fn from(line: SourceLine) -> Self {
        match line.path {
            Some(path) => Self::Checkout { client: line.client, path },
            None => Self::Web,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReferenceLine {
    Message { event: EventId, quote: String },
    File { path: PathBuf, lines: (u32, u32), excerpt: String },
}

impl From<&Reference> for ReferenceLine {
    fn from(reference: &Reference) -> Self {
        match reference {
            Reference::Message { event, quote } => Self::Message { event: *event, quote: quote.clone() },
            Reference::File { path, lines, excerpt } => Self::File { path: path.clone(), lines: *lines, excerpt: excerpt.clone() },
        }
    }
}

impl From<ReferenceLine> for Reference {
    fn from(line: ReferenceLine) -> Self {
        match line {
            ReferenceLine::Message { event, quote } => Self::Message { event, quote },
            ReferenceLine::File { path, lines, excerpt } => Self::File { path, lines, excerpt },
        }
    }
}

fn field<T: DeserializeOwned>(
    payload: &serde_json::Value,
    name: &str,
) -> Result<T> {
    serde_json::from_value(payload[name].clone())
        .map_err(|error| format!("payload.{name}: {error}").into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::core::{BoardId, MapId, NodeId, ProjectId, SessionId};

    #[test]
    fn every_payload_round_trips_through_a_line() {
        let session = SessionId::new();
        let raw = json!({"r": 1});
        let events = [
            Event::session_created(Source::fixture(), session, ProjectId::new(), ClientSessionId::new("a".into())),
            Event::session_started(Source::fixture(), session, raw.clone()),
            Event::session_stopped(Source::fixture(), session, raw.clone()),
            Event::message(Source::fixture(), session, Actor::Human, "hi", raw.clone()),
            Event::tool_used(Source::fixture(), session, "Bash", Some("ls".into()), Some("a.txt".into()), raw.clone()),
            Event::tool_used(Source::fixture(), session, "Read", None, None, raw.clone()),
            Event::subagent_started(Source::fixture(), session, ClientAgentId::new("a1".into()), "builder", "Build it", raw.clone()),
            Event::subagent_reported(Source::fixture(), session, ClientAgentId::new("a1".into()), "done", raw),
            Event::project_created(Source::fixture(), Actor::Human, ProjectId::new(), Name::new("percept").unwrap(), Some("/work/percept".into())),
            Event::board_created(Source::fixture(), Actor::Human, BoardId::new(), ProjectId::new(), Name::new("ideas").unwrap()),
            Event::map_created(Source::fixture(), Actor::Human, MapId::new(), ProjectId::new(), "decisions", "/maps/decisions.toml".into()),
            Event::node_added(Source::fixture(), Actor::Agent, MapId::new(), NodeId::new(), "question", "Why?", BTreeMap::from([("definition".into(), "D".into())]), Vec::new()),
            Event::node_changed(Source::fixture(), Actor::Human, MapId::new(), NodeId::new(), "Why not?", BTreeMap::from([("definition".into(), "E".into())]), Vec::new()),
            Event::node_removed(Source::fixture(), Actor::Human, MapId::new(), NodeId::new()),
            Event::edge_added(Source::fixture(), Actor::Agent, MapId::new(), "answers", NodeId::new(), NodeId::new()),
            Event::edge_removed(Source::fixture(), Actor::Human, MapId::new(), "answers", NodeId::new(), NodeId::new()),
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
    fn a_web_source_is_a_line_without_a_path() {
        let event = Event::board_created(Source::Web, Actor::Human, BoardId::new(), ProjectId::new(), Name::new("ideas").unwrap());

        let value = serde_json::to_value(EventLine::from(&event)).unwrap();
        let line: EventLine = serde_json::from_value(value.clone()).unwrap();
        let back = Event::try_from(line).unwrap();

        assert_eq!(value["source"], json!({"client": "percept-web"}));
        assert_eq!(back.source, Source::Web);
    }

    #[test]
    fn a_project_without_a_root_is_a_null_root_and_a_missing_root_reads_the_same() {
        let event = Event::project_created(Source::Web, Actor::Human, ProjectId::new(), Name::new("notes").unwrap(), None);
        let mut line = EventLine::from(&event);
        assert_eq!(line.payload["root"], json!(null));

        line.payload.as_object_mut().unwrap().remove("root");
        let back = Event::try_from(line).unwrap();

        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn a_node_line_without_properties_reads_as_empty() {
        let (map, node) = (MapId::new(), NodeId::new());
        let event = Event::node_added(Source::fixture(), Actor::Agent, map, node, "question", "Why?", BTreeMap::new(), Vec::new());
        let mut line = EventLine::from(&event);
        line.payload = json!({ "map": map, "node": node, "kind": "question", "title": "Why?" });

        let back = Event::try_from(line).unwrap();

        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn a_node_changed_line_is_the_full_new_state() {
        let (map, node, event_id) = (MapId::new(), NodeId::new(), EventId::new());
        let references = vec![Reference::Message { event: event_id, quote: "Use one log.".into() }];
        let properties = BTreeMap::from([("definition".to_string(), "D".to_string())]);
        let event = Event::node_changed(Source::fixture(), Actor::Human, map, node, "One log", properties, references);

        let value = serde_json::to_value(EventLine::from(&event)).unwrap();
        let line: EventLine = serde_json::from_value(value.clone()).unwrap();
        let back = Event::try_from(line).unwrap();

        assert_eq!(value["kind"], "node.changed");
        assert_eq!(value["payload"], json!({
            "map": map,
            "node": node,
            "title": "One log",
            "properties": { "definition": "D" },
            "references": [{ "message": { "event": event_id, "quote": "Use one log." } }],
        }));
        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn a_node_removed_line_names_the_node() {
        let (map, node) = (MapId::new(), NodeId::new());
        let event = Event::node_removed(Source::fixture(), Actor::Human, map, node);

        let value = serde_json::to_value(EventLine::from(&event)).unwrap();
        let line: EventLine = serde_json::from_value(value.clone()).unwrap();
        let back = Event::try_from(line).unwrap();

        assert_eq!(value["kind"], "node.removed");
        assert_eq!(value["payload"], json!({ "map": map, "node": node }));
        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn an_edge_removed_line_names_the_edge() {
        let (map, from, to) = (MapId::new(), NodeId::new(), NodeId::new());
        let event = Event::edge_removed(Source::fixture(), Actor::Human, map, "answers", from, to);

        let value = serde_json::to_value(EventLine::from(&event)).unwrap();
        let line: EventLine = serde_json::from_value(value.clone()).unwrap();
        let back = Event::try_from(line).unwrap();

        assert_eq!(value["kind"], "edge.removed");
        assert_eq!(value["payload"], json!({ "map": map, "kind": "answers", "from": from, "to": to }));
        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn a_tool_line_without_input_and_output_reads_as_none() {
        let session = SessionId::new();
        let event = Event::tool_used(Source::fixture(), session, "Bash", None, None, json!({}));
        let mut line = EventLine::from(&event);
        line.payload = json!({ "session": session, "tool": "Bash" });

        let back = Event::try_from(line).unwrap();

        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn node_references_round_trip_in_the_flat_payload() {
        let event_id = EventId::new();
        let references = vec![
            Reference::Message { event: event_id, quote: "Use one log.".into() },
            Reference::File { path: "src/main.rs".into(), lines: (2, 4), excerpt: "fn main() {}\n".into() },
        ];
        let event = Event::node_added(
            Source::fixture(), Actor::Human, MapId::new(), NodeId::new(),
            "decision", "One log", BTreeMap::new(), references,
        );

        let value = serde_json::to_value(EventLine::from(&event)).unwrap();
        let line: EventLine = serde_json::from_value(value.clone()).unwrap();
        let back = Event::try_from(line).unwrap();

        assert_eq!(value["payload"]["kind"], "decision");
        assert_eq!(value["payload"]["title"], "One log");
        assert_eq!(value["payload"]["references"], json!([
            { "message": { "event": event_id, "quote": "Use one log." } },
            { "file": { "path": "src/main.rs", "lines": [2, 4], "excerpt": "fn main() {}\n" } },
        ]));
        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn node_lines_write_empty_references_and_read_a_missing_field_as_empty() {
        let event = Event::node_added(
            Source::fixture(), Actor::Human, MapId::new(), NodeId::new(),
            "question", "Why?", BTreeMap::new(), Vec::new(),
        );
        let mut line = EventLine::from(&event);
        assert_eq!(line.payload["references"], json!([]));
        line.payload.as_object_mut().unwrap().remove("references");

        let back = Event::try_from(line).unwrap();

        assert_eq!(back.payload, event.payload);
    }

    #[test]
    fn malformed_node_references_are_errors() {
        let malformed = [
            json!(null),
            json!({}),
            json!([{}]),
            json!([{ "unknown": {} }]),
            json!([{ "message": { "event": "invalid", "quote": "Q" } }]),
            json!([{ "message": { "event": EventId::new() } }]),
            json!([{ "file": { "path": "src/main.rs", "lines": [1, 2] } }]),
            json!([{ "file": { "path": "src/main.rs", "lines": [1], "excerpt": "Q" } }]),
            json!([{ "file": { "path": "src/main.rs", "lines": [-1, 2], "excerpt": "Q" } }]),
        ];

        for references in malformed {
            let event = Event::node_added(
                Source::fixture(), Actor::Human, MapId::new(), NodeId::new(),
                "question", "Why?", BTreeMap::new(), Vec::new(),
            );
            let mut line = EventLine::from(&event);
            line.payload["references"] = references;

            let error = Event::try_from(line).unwrap_err();

            assert!(error.to_string().contains("payload.references"));
        }
    }

    #[test]
    fn a_line_has_a_kind_and_a_payload_naming_the_session() {
        let session = SessionId::new();
        let event = Event::message(Source::fixture(), session, Actor::Human, "hi", json!({"r": 1}));

        let line = serde_json::to_value(EventLine::from(&event)).unwrap();

        assert_eq!(line["source"], json!({"client": "claude-code", "path": "/repo"}));
        assert_eq!(line["actor"], "human");
        assert_eq!(line["kind"], "message");
        assert_eq!(line["payload"], json!({"session": session, "content": "hi"}));
        assert_eq!(line["raw"], json!({"r": 1}));
    }

    #[test]
    fn subagent_lines_name_the_subagent() {
        let (session, subagent) = (SessionId::new(), ClientAgentId::new("a1".into()));
        let started = Event::subagent_started(Source::fixture(), session, subagent.clone(), "builder", "Build it", json!({}));
        let reported = Event::subagent_reported(Source::fixture(), session, subagent, "done", json!({}));

        let started = serde_json::to_value(EventLine::from(&started)).unwrap();
        let reported = serde_json::to_value(EventLine::from(&reported)).unwrap();

        assert_eq!(started["kind"], "subagent.started");
        assert_eq!(started["payload"], json!({"session": session, "subagent": "a1", "agent": "builder", "task": "Build it"}));
        assert_eq!(reported["kind"], "subagent.reported");
        assert_eq!(reported["payload"], json!({"session": session, "subagent": "a1", "content": "done"}));
    }

    #[test]
    fn unknown_kind_is_an_error() {
        let event = Event::message(Source::fixture(), SessionId::new(), Actor::Human, "hi", json!({}));
        let mut line = EventLine::from(&event);
        line.kind = "bogus".into();

        assert!(Event::try_from(line).is_err());
    }
}
