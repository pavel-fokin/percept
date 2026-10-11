use jiff::Timestamp;
use serde::Serialize;

use crate::shared::Id;

/// A conversation with a coding client, created by a `SessionCreated` event.
#[derive(Serialize)]
pub struct Session {
    pub id: SessionId,
    pub client_session_id: ClientSessionId,
    pub created_at: Timestamp,
    /// The first non-empty thing the human said, if they said anything.
    pub title: Option<String>,
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
