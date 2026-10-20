use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::json;

use super::paths::{display_path, display_paths};
use crate::app::CitedMessage;
use crate::core::{self, Actor, ClientSessionId, EventId, MapId, NodeId, Payload, SessionId};

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
    pub(super) id: MapId,
    pub(super) kind: String,
    pub(super) path: String,
    pub(super) nodes: usize,
}

/// A whole map as the page reads it, with its path as a human sees it.
#[derive(Serialize)]
pub(super) struct MapDetail {
    pub(super) id: MapId,
    pub(super) kind: String,
    pub(super) path: String,
    pub(super) nodes: Vec<Node>,
    pub(super) edges: Vec<Edge>,
}

#[derive(Serialize)]
pub(super) struct Node {
    pub(super) id: NodeId,
    pub(super) kind: String,
    pub(super) title: String,
    pub(super) properties: BTreeMap<String, String>,
    pub(super) references: Vec<Reference>,
    pub(super) created_at: jiff::Timestamp,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Reference {
    Message { event: EventId, session: SessionId, created_at: jiff::Timestamp, quote: String },
    File { path: String, lines: (u32, u32), excerpt: String },
}

impl Reference {
    /// A node's reference is validated when written, so its message is in `messages`.
    pub(super) fn new(reference: core::Reference, messages: &BTreeMap<EventId, CitedMessage>) -> Self {
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
pub(super) struct Edge {
    pub(super) kind: String,
    pub(super) from: NodeId,
    pub(super) to: NodeId,
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
        }
    }
}

/// An event as the page reads it: the hook input stays in the log.
#[derive(Serialize)]
pub(super) struct Event {
    pub(super) id: EventId,
    pub(super) actor: Actor,
    pub(super) kind: &'static str,
    pub(super) payload: serde_json::Value,
    pub(super) created_at: jiff::Timestamp,
}

pub(super) fn view_payload(payload: &Payload, home: &Path) -> serde_json::Value {
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
