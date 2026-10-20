use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::json;

use super::paths::{display_path, display_paths};
use crate::app::{self, CitedMessage};
use crate::core::{self, Actor, ClientSessionId, EventId, MapId, NodeId, Payload, SessionId, SessionStatus};

/// The envelope every API response is sent in.
#[derive(Serialize)]
pub(super) struct Data<T> {
    pub(super) data: T,
}

#[derive(Serialize)]
pub(super) struct Status {
    pub(super) events: usize,
}

/// A map as the page reads it, with its path as a human sees it.
#[derive(Serialize)]
pub(super) struct MapSummary {
    id: MapId,
    kind: String,
    path: String,
    nodes: usize,
}

impl MapSummary {
    pub(super) fn new(summary: app::MapSummary, home: &Path) -> Self {
        Self {
            id: summary.map.id,
            kind: summary.map.kind,
            path: display_path(&summary.map.path, home),
            nodes: summary.nodes,
        }
    }
}

/// A whole map as the page reads it, with its path as a human sees it.
#[derive(Serialize)]
pub(super) struct MapDetail {
    id: MapId,
    kind: String,
    path: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl MapDetail {
    pub(super) fn new(detail: app::MapDetail, home: &Path) -> Self {
        let messages = &detail.messages;

        Self {
            id: detail.map.id,
            kind: detail.map.kind,
            path: display_path(&detail.map.path, home),
            nodes: detail.nodes.into_iter().map(|n| Node::new(n, messages)).collect(),
            edges: detail.edges.into_iter().map(Edge::from).collect(),
        }
    }
}

#[derive(Serialize)]
struct Node {
    id: NodeId,
    kind: String,
    title: String,
    properties: BTreeMap<String, String>,
    references: Vec<Reference>,
    created_at: jiff::Timestamp,
}

impl Node {
    fn new(node: core::Node, messages: &BTreeMap<EventId, CitedMessage>) -> Self {
        Self {
            id: node.id,
            kind: node.kind,
            title: node.title,
            properties: node.properties,
            references: node.references.into_iter().map(|r| Reference::new(r, messages)).collect(),
            created_at: node.created_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Reference {
    Message { event: EventId, session: SessionId, created_at: jiff::Timestamp, quote: String },
    File { path: String, lines: (u32, u32), excerpt: String },
}

impl Reference {
    /// A node's reference is validated when written, so its message is in `messages`.
    fn new(reference: core::Reference, messages: &BTreeMap<EventId, CitedMessage>) -> Self {
        match reference {
            core::Reference::Message { event, quote } => {
                let CitedMessage { session, created_at } = messages[&event];
                Self::Message { event, session, created_at, quote }
            }
            core::Reference::File { path, lines, excerpt } => Self::File { path: path.display().to_string(), lines, excerpt },
        }
    }
}

#[derive(Serialize)]
struct Edge {
    kind: String,
    from: NodeId,
    to: NodeId,
}

impl From<core::Edge> for Edge {
    fn from(edge: core::Edge) -> Self {
        Self { kind: edge.kind, from: edge.from, to: edge.to }
    }
}

/// A session as the page reads it, with its path as a human sees it.
#[derive(Serialize)]
pub(super) struct Session {
    id: SessionId,
    client_session_id: ClientSessionId,
    created_at: jiff::Timestamp,
    title: Option<String>,
    path: String,
    client: String,
    status: SessionStatus,
    last_event_at: jiff::Timestamp,
}

impl Session {
    pub(super) fn new(session: core::Session, home: &Path) -> Self {
        Self {
            id: session.id,
            client_session_id: session.client_session_id,
            created_at: session.created_at,
            title: session.title,
            path: display_path(&session.path, home),
            client: session.client,
            status: session.status,
            last_event_at: session.last_event_at,
        }
    }
}

/// An event as the page reads it: the hook input stays in the log.
#[derive(Serialize)]
pub(super) struct Event {
    id: EventId,
    actor: Actor,
    kind: &'static str,
    payload: serde_json::Value,
    created_at: jiff::Timestamp,
}

impl Event {
    pub(super) fn new(event: core::Event, home: &Path) -> Self {
        Self {
            id: event.id,
            actor: event.actor,
            kind: event.payload.kind().name(),
            payload: payload(&event.payload, home),
            created_at: event.created_at,
        }
    }
}

fn payload(payload: &Payload, home: &Path) -> serde_json::Value {
    match payload {
        Payload::Message { content, .. } => json!({ "content": content }),
        Payload::ToolUsed { tool, input, output, .. } => {
            let input = input.as_deref().map(|input| display_paths(input, home));
            json!({ "tool": tool, "input": input, "output": output })
        }
        Payload::SubagentStarted { subagent, agent, task, .. } => {
            json!({ "subagent": subagent.as_str(), "agent": agent, "task": task })
        }
        Payload::SubagentReported { subagent, content, .. } => {
            json!({ "subagent": subagent.as_str(), "content": content })
        }
        Payload::SessionCreated { .. }
        | Payload::SessionStarted { .. }
        | Payload::SessionStopped { .. }
        | Payload::MapCreated { .. }
        | Payload::NodeAdded { .. }
        | Payload::EdgeAdded { .. } => json!({}),
    }
}
