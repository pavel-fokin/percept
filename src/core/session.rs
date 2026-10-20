use std::path::PathBuf;

use jiff::Timestamp;
use serde::Serialize;

use super::{Actor, Event, Payload};
use crate::shared::Id;

/// A conversation with a coding client, created by a `SessionCreated` event.
pub struct Session {
    pub id: SessionId,
    pub client_session_id: ClientSessionId,
    pub created_at: Timestamp,
    /// Where the client ran, from the `source` of the `SessionCreated` event.
    pub path: PathBuf,
    /// The client that ran it, from the same event.
    pub client: String,
    /// The first non-empty thing the human said, if they said anything.
    pub title: Option<String>,
    pub status: SessionStatus,
    pub last_event_at: Timestamp,
}

/// What the session is doing, from its last event.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Working,
    Waiting,
    Ended,
}

impl SessionStatus {
    /// The status a session is in after `event`; map events change none. A hook sends an agent message only when the agent stops.
    pub fn after(event: &Event) -> Option<Self> {
        match &event.payload {
            Payload::SessionStopped { .. } => Some(Self::Ended),
            Payload::SessionCreated { .. } | Payload::SessionStarted { .. } => Some(Self::Waiting),
            Payload::Message { .. } if event.actor == Actor::Agent => Some(Self::Waiting),
            Payload::Message { .. } | Payload::ToolUsed { .. } | Payload::SubagentStarted { .. } | Payload::SubagentReported { .. } => {
                Some(Self::Working)
            }
            Payload::MapCreated { .. } | Payload::NodeAdded { .. } | Payload::EdgeAdded { .. } => None,
        }
    }
}

pub type SessionId = Id<Session>;

/// The client's own session identifier, as the client sent it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClientSessionId(String);

impl ClientSessionId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The client's own identifier for a subagent, as the client sent it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClientAgentId(String);

impl ClientAgentId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
