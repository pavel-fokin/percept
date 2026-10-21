use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::json;

use super::paths::{display_path, display_paths};
use crate::app::{self, CitedMessage};
use crate::core::{self, Actor, ClientSessionId, EventId, NodeId, Payload, Project, ProjectId, SessionId, SessionStatus};

/// The envelope every API response is sent in.
#[derive(Serialize)]
pub(super) struct Data<T> {
    pub(super) data: T,
}

#[derive(Serialize)]
pub(super) struct Status {
    pub(super) events: usize,
}

/// The name of the project with this id. Every session and map names a project that was created.
fn project_name(projects: &[Project], id: ProjectId) -> String {
    projects.iter().find(|p| p.id == id).map(|p| p.name.as_str().to_string()).expect("a project was created before what it holds")
}

/// A project as the page reads it: its name and its root shown from home.
#[derive(Serialize)]
pub(super) struct ProjectSummary {
    name: String,
    root: String,
}

impl ProjectSummary {
    pub(super) fn new(project: Project, home: &Path) -> Self {
        Self { name: project.name.as_str().to_string(), root: display_path(&project.root, home) }
    }
}

/// A map as the page reads it, under the name of its project.
#[derive(Serialize)]
pub(super) struct MapSummary {
    kind: String,
    project: String,
    nodes: usize,
}

impl MapSummary {
    pub(super) fn new(map: core::Map, projects: &[Project]) -> Self {
        Self { kind: map.kind, project: project_name(projects, map.project), nodes: map.nodes.len() }
    }
}

/// A board as the page reads it, under the name of its project.
#[derive(Serialize)]
pub(super) struct BoardSummary {
    id: core::BoardId,
    name: String,
    project: String,
}

impl BoardSummary {
    pub(super) fn new(board: core::Board, projects: &[Project]) -> Self {
        Self { id: board.id, name: board.name.as_str().to_string(), project: project_name(projects, board.project) }
    }
}

/// A whole map as the page reads it, under the name of its project.
#[derive(Serialize)]
pub(super) struct MapDetail {
    kind: String,
    project: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl MapDetail {
    pub(super) fn new(detail: app::MapDetail, project: String) -> Self {
        let (map, messages) = (detail.map, &detail.messages);

        Self {
            kind: map.kind,
            project,
            nodes: map.nodes.into_iter().map(|n| Node::new(n, messages)).collect(),
            edges: map.edges.into_iter().map(Edge::from).collect(),
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
    /// When the node last changed; none while it is as it was added.
    changed_at: Option<jiff::Timestamp>,
}

impl Node {
    fn new(node: core::Node, messages: &BTreeMap<EventId, CitedMessage>) -> Self {
        Self {
            id: node.id,
            kind: node.kind,
            title: node.title,
            properties: node.properties,
            references: node.references.into_iter().map(|r| Reference::new(r, messages)).collect(),
            created_at: node.changes[0].at,
            changed_at: node.changes[1..].last().map(|c| c.at),
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

/// A session as the page reads it, under the name of its project.
#[derive(Serialize)]
pub(super) struct Session {
    id: SessionId,
    client_session_id: ClientSessionId,
    created_at: jiff::Timestamp,
    title: Option<String>,
    project: String,
    client: String,
    status: SessionStatus,
    last_event_at: jiff::Timestamp,
}

impl Session {
    pub(super) fn new(session: core::Session, projects: &[Project]) -> Self {
        Self {
            id: session.id,
            client_session_id: session.client_session_id,
            created_at: session.created_at,
            title: session.title,
            project: project_name(projects, session.project),
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
        | Payload::ProjectCreated { .. }
        | Payload::BoardCreated { .. }
        | Payload::MapCreated { .. }
        | Payload::NodeAdded { .. }
        | Payload::NodeChanged { .. }
        | Payload::NodeRemoved { .. }
        | Payload::EdgeAdded { .. }
        | Payload::EdgeRemoved { .. } => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MapId, Source};

    #[test]
    fn a_node_has_a_changed_time_only_after_a_change() {
        let (map, node) = (MapId::new(), NodeId::new());
        let added = core::Event::node_added(Source::fixture(), Actor::Agent, map, node, "question", "Q", BTreeMap::new(), Vec::new());
        let changed = core::Event::node_changed(Source::fixture(), Actor::Human, map, node, "Q2", BTreeMap::new(), Vec::new());
        let response = |events: &[core::Event]| {
            let node = core::Map::fixture(map, events).nodes.remove(0);
            serde_json::to_value(Node::new(node, &BTreeMap::new())).unwrap()
        };

        let as_added = response(std::slice::from_ref(&added));
        let as_changed = response(&[added.clone(), changed.clone()]);

        assert_eq!(as_added["created_at"], json!(added.created_at));
        assert_eq!(as_added["changed_at"], json!(null));
        assert_eq!(as_changed["created_at"], json!(added.created_at));
        assert_eq!(as_changed["changed_at"], json!(changed.created_at));
    }
}
