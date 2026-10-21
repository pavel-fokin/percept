use std::collections::BTreeMap;
use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{BoardId, ClientAgentId, ClientSessionId, MapId, Name, NodeId, ProjectId, Reference, SessionId};
use crate::shared::Id;

pub type EventId = Id<Event>;

/// Who an event is attributed to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Human,
    Agent,
    System,
}

/// Where an event came from: the coding client and the folder it ran in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub client: String,
    pub path: PathBuf,
}

#[cfg(test)]
impl Source {
    pub fn fixture() -> Self {
        Self { client: "claude-code".into(), path: "/repo".into() }
    }
}

/// What kind of thing happened. Its name is how the kind is written down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    SessionCreated,
    SessionStarted,
    SessionStopped,
    Message,
    ToolUsed,
    SubagentStarted,
    SubagentReported,
    ProjectCreated,
    BoardCreated,
    MapCreated,
    NodeAdded,
    NodeChanged,
    NodeRemoved,
    EdgeAdded,
    EdgeRemoved,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::SessionCreated => "session.created",
            Self::SessionStarted => "session.started",
            Self::SessionStopped => "session.stopped",
            Self::Message => "message",
            Self::ToolUsed => "tool.used",
            Self::SubagentStarted => "subagent.started",
            Self::SubagentReported => "subagent.reported",
            Self::ProjectCreated => "project.created",
            Self::BoardCreated => "board.created",
            Self::MapCreated => "map.created",
            Self::NodeAdded => "node.added",
            Self::NodeChanged => "node.changed",
            Self::NodeRemoved => "node.removed",
            Self::EdgeAdded => "edge.added",
            Self::EdgeRemoved => "edge.removed",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "session.created" => Some(Self::SessionCreated),
            "session.started" => Some(Self::SessionStarted),
            "session.stopped" => Some(Self::SessionStopped),
            "message" => Some(Self::Message),
            "tool.used" => Some(Self::ToolUsed),
            "subagent.started" => Some(Self::SubagentStarted),
            "subagent.reported" => Some(Self::SubagentReported),
            "project.created" => Some(Self::ProjectCreated),
            "board.created" => Some(Self::BoardCreated),
            "map.created" => Some(Self::MapCreated),
            "node.added" => Some(Self::NodeAdded),
            "node.changed" => Some(Self::NodeChanged),
            "node.removed" => Some(Self::NodeRemoved),
            "edge.added" => Some(Self::EdgeAdded),
            "edge.removed" => Some(Self::EdgeRemoved),
            _ => None,
        }
    }
}

/// What happened, with the data that kind of event carries.
#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
    SessionCreated { session: SessionId, project: ProjectId, client_session_id: ClientSessionId },
    SessionStarted { session: SessionId },
    SessionStopped { session: SessionId },
    Message { session: SessionId, content: String },
    ToolUsed { session: SessionId, tool: String, input: Option<String>, output: Option<String> },
    SubagentStarted { session: SessionId, subagent: ClientAgentId, agent: String, task: String },
    SubagentReported { session: SessionId, subagent: ClientAgentId, content: String },
    ProjectCreated { project: ProjectId, name: Name },
    BoardCreated { board: BoardId, project: ProjectId, name: Name },
    MapCreated { map: MapId, project: ProjectId, kind: String },
    NodeAdded { map: MapId, node: NodeId, kind: String, title: String, properties: BTreeMap<String, String>, references: Vec<Reference> },
    /// The node's full new state, not a diff.
    NodeChanged { map: MapId, node: NodeId, title: String, properties: BTreeMap<String, String>, references: Vec<Reference> },
    NodeRemoved { map: MapId, node: NodeId },
    EdgeAdded { map: MapId, kind: String, from: NodeId, to: NodeId },
    EdgeRemoved { map: MapId, kind: String, from: NodeId, to: NodeId },
}

impl Payload {
    pub fn kind(&self) -> Kind {
        match self {
            Self::SessionCreated { .. } => Kind::SessionCreated,
            Self::SessionStarted { .. } => Kind::SessionStarted,
            Self::SessionStopped { .. } => Kind::SessionStopped,
            Self::Message { .. } => Kind::Message,
            Self::ToolUsed { .. } => Kind::ToolUsed,
            Self::SubagentStarted { .. } => Kind::SubagentStarted,
            Self::SubagentReported { .. } => Kind::SubagentReported,
            Self::ProjectCreated { .. } => Kind::ProjectCreated,
            Self::BoardCreated { .. } => Kind::BoardCreated,
            Self::MapCreated { .. } => Kind::MapCreated,
            Self::NodeAdded { .. } => Kind::NodeAdded,
            Self::NodeChanged { .. } => Kind::NodeChanged,
            Self::NodeRemoved { .. } => Kind::NodeRemoved,
            Self::EdgeAdded { .. } => Kind::EdgeAdded,
            Self::EdgeRemoved { .. } => Kind::EdgeRemoved,
        }
    }

    pub fn session(&self) -> Option<SessionId> {
        match self {
            Self::SessionCreated { session, .. }
            | Self::SessionStarted { session }
            | Self::SessionStopped { session }
            | Self::Message { session, .. }
            | Self::ToolUsed { session, .. }
            | Self::SubagentStarted { session, .. }
            | Self::SubagentReported { session, .. } => Some(*session),
            Self::ProjectCreated { .. } | Self::BoardCreated { .. } | Self::MapCreated { .. } | Self::NodeAdded { .. } | Self::NodeChanged { .. } | Self::NodeRemoved { .. } | Self::EdgeAdded { .. } | Self::EdgeRemoved { .. } => None,
        }
    }

    pub fn map(&self) -> Option<MapId> {
        match self {
            Self::MapCreated { map, .. }
            | Self::NodeAdded { map, .. }
            | Self::NodeChanged { map, .. }
            | Self::NodeRemoved { map, .. }
            | Self::EdgeAdded { map, .. }
            | Self::EdgeRemoved { map, .. } => Some(*map),
            Self::ProjectCreated { .. }
            | Self::BoardCreated { .. }
            | Self::SessionCreated { .. }
            | Self::SessionStarted { .. }
            | Self::SessionStopped { .. }
            | Self::Message { .. }
            | Self::ToolUsed { .. }
            | Self::SubagentStarted { .. }
            | Self::SubagentReported { .. } => None,
        }
    }
}

/// `raw` is the hook input, unchanged. Events percept makes itself have no `raw`.
#[derive(Debug, Clone)]
pub struct Event {
    pub id: EventId,
    pub source: Source,
    pub actor: Actor,
    pub payload: Payload,
    pub raw: Option<serde_json::Value>,
    pub created_at: Timestamp,
}

impl Event {
    pub fn session_created(source: Source, session: SessionId, project: ProjectId, client_session_id: ClientSessionId) -> Self {
        Self::build(source, Actor::System, Payload::SessionCreated { session, project, client_session_id }, None)
    }

    pub fn message(
        source: Source,
        session: SessionId,
        actor: Actor,
        content: &str,
        raw: serde_json::Value,
    ) -> Self {
        let payload = Payload::Message { session, content: content.into() };
        Self::build(source, actor, payload, Some(raw))
    }

    pub fn tool_used(
        source: Source,
        session: SessionId,
        tool: &str,
        input: Option<String>,
        output: Option<String>,
        raw: serde_json::Value,
    ) -> Self {
        let payload = Payload::ToolUsed { session, tool: tool.into(), input, output };
        Self::build(source, Actor::Agent, payload, Some(raw))
    }

    pub fn subagent_started(
        source: Source,
        session: SessionId,
        subagent: ClientAgentId,
        agent: &str,
        task: &str,
        raw: serde_json::Value,
    ) -> Self {
        let payload = Payload::SubagentStarted { session, subagent, agent: agent.into(), task: task.into() };
        Self::build(source, Actor::Agent, payload, Some(raw))
    }

    pub fn subagent_reported(
        source: Source,
        session: SessionId,
        subagent: ClientAgentId,
        content: &str,
        raw: serde_json::Value,
    ) -> Self {
        let payload = Payload::SubagentReported { session, subagent, content: content.into() };
        Self::build(source, Actor::Agent, payload, Some(raw))
    }

    pub fn session_started(source: Source, session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(source, Actor::System, Payload::SessionStarted { session }, Some(raw))
    }

    pub fn session_stopped(source: Source, session: SessionId, raw: serde_json::Value) -> Self {
        Self::build(source, Actor::System, Payload::SessionStopped { session }, Some(raw))
    }

    pub fn project_created(source: Source, actor: Actor, project: ProjectId, name: Name) -> Self {
        Self::build(source, actor, Payload::ProjectCreated { project, name }, None)
    }

    pub fn board_created(source: Source, actor: Actor, board: BoardId, project: ProjectId, name: Name) -> Self {
        Self::build(source, actor, Payload::BoardCreated { board, project, name }, None)
    }

    pub fn map_created(source: Source, actor: Actor, map: MapId, project: ProjectId, kind: &str) -> Self {
        let payload = Payload::MapCreated { map, project, kind: kind.into() };
        Self::build(source, actor, payload, None)
    }

    #[allow(clippy::too_many_arguments, reason = "the fields of node.added")]
    pub fn node_added(
        source: Source,
        actor: Actor,
        map: MapId,
        node: NodeId,
        kind: &str,
        title: &str,
        properties: BTreeMap<String, String>,
        references: Vec<Reference>,
    ) -> Self {
        let payload = Payload::NodeAdded { map, node, kind: kind.into(), title: title.into(), properties, references };
        Self::build(source, actor, payload, None)
    }

    pub fn node_changed(
        source: Source,
        actor: Actor,
        map: MapId,
        node: NodeId,
        title: &str,
        properties: BTreeMap<String, String>,
        references: Vec<Reference>,
    ) -> Self {
        let payload = Payload::NodeChanged { map, node, title: title.into(), properties, references };
        Self::build(source, actor, payload, None)
    }

    pub fn edge_added(source: Source, actor: Actor, map: MapId, kind: &str, from: NodeId, to: NodeId) -> Self {
        let payload = Payload::EdgeAdded { map, kind: kind.into(), from, to };
        Self::build(source, actor, payload, None)
    }

    pub fn node_removed(source: Source, actor: Actor, map: MapId, node: NodeId) -> Self {
        let payload = Payload::NodeRemoved { map, node };
        Self::build(source, actor, payload, None)
    }

    pub fn edge_removed(source: Source, actor: Actor, map: MapId, kind: &str, from: NodeId, to: NodeId) -> Self {
        let payload = Payload::EdgeRemoved { map, kind: kind.into(), from, to };
        Self::build(source, actor, payload, None)
    }

    fn build(source: Source, actor: Actor, payload: Payload, raw: Option<serde_json::Value>) -> Self {
        Self {
            id: EventId::new(),
            source,
            actor,
            payload,
            raw,
            created_at: Timestamp::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_created_is_a_system_event_carrying_the_client_session_id() {
        let event = Event::session_created(Source::fixture(), SessionId::new(), ProjectId::new(), ClientSessionId::new("a".into()));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload.kind(), Kind::SessionCreated);
        assert!(matches!(
            &event.payload,
            Payload::SessionCreated { client_session_id, .. } if client_session_id.as_str() == "a"
        ));
        assert_eq!(event.raw, None);
    }

    #[test]
    fn message_carries_the_session_actor_and_content() {
        let session = SessionId::new();

        let event = Event::message(Source::fixture(), session, Actor::Human, "hi", json!({"r": 1}));

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload, Payload::Message { session, content: "hi".into() });
        assert_eq!(event.raw, Some(json!({"r": 1})));
    }

    #[test]
    fn tool_used_is_an_agent_event_carrying_the_tool() {
        let session = SessionId::new();

        let event = Event::tool_used(Source::fixture(), session, "Bash", Some("ls".into()), None, json!({"r": 2}));

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::ToolUsed { session, tool: "Bash".into(), input: Some("ls".into()), output: None });
        assert_eq!(event.raw, Some(json!({"r": 2})));
    }

    #[test]
    fn subagent_started_is_an_agent_event_carrying_the_subagent_agent_and_task() {
        let session = SessionId::new();
        let subagent = ClientAgentId::new("a1".into());

        let event = Event::subagent_started(Source::fixture(), session, subagent.clone(), "builder", "Build it", json!({"r": 5}));

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(
            event.payload,
            Payload::SubagentStarted { session, subagent, agent: "builder".into(), task: "Build it".into() }
        );
        assert_eq!(event.raw, Some(json!({"r": 5})));
    }

    #[test]
    fn subagent_reported_is_an_agent_event_carrying_the_subagent_and_content() {
        let session = SessionId::new();
        let subagent = ClientAgentId::new("a1".into());

        let event = Event::subagent_reported(Source::fixture(), session, subagent.clone(), "done", json!({"r": 6}));

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::SubagentReported { session, subagent, content: "done".into() });
        assert_eq!(event.raw, Some(json!({"r": 6})));
    }

    #[test]
    fn session_started_is_a_system_event() {
        let session = SessionId::new();

        let event = Event::session_started(Source::fixture(), session, json!({"r": 3}));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::SessionStarted { session });
        assert_eq!(event.raw, Some(json!({"r": 3})));
    }

    #[test]
    fn session_stopped_is_a_system_event() {
        let session = SessionId::new();

        let event = Event::session_stopped(Source::fixture(), session, json!({"r": 4}));

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::SessionStopped { session });
        assert_eq!(event.raw, Some(json!({"r": 4})));
    }

    #[test]
    fn every_payload_names_its_kind_and_session() {
        let session = SessionId::new();
        let payloads = [
            (Payload::SessionCreated { session, project: ProjectId::new(), client_session_id: ClientSessionId::new("a".into()) }, Kind::SessionCreated),
            (Payload::SessionStarted { session }, Kind::SessionStarted),
            (Payload::SessionStopped { session }, Kind::SessionStopped),
            (Payload::Message { session, content: "hi".into() }, Kind::Message),
            (Payload::ToolUsed { session, tool: "Bash".into(), input: None, output: None }, Kind::ToolUsed),
            (
                Payload::SubagentStarted { session, subagent: ClientAgentId::new("a1".into()), agent: "builder".into(), task: "Build it".into() },
                Kind::SubagentStarted,
            ),
            (
                Payload::SubagentReported { session, subagent: ClientAgentId::new("a1".into()), content: "done".into() },
                Kind::SubagentReported,
            ),
        ];

        for (payload, kind) in payloads {
            assert_eq!(payload.kind(), kind);
            assert_eq!(payload.session(), Some(session));
        }
    }

    #[test]
    fn map_created_carries_the_actor_and_has_no_session() {
        let (map, project) = (MapId::new(), ProjectId::new());

        let event = Event::map_created(Source::fixture(), Actor::Human, map, project, "decisions");

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload.kind(), Kind::MapCreated);
        assert_eq!(event.payload, Payload::MapCreated { map, project, kind: "decisions".into() });
        assert_eq!(event.payload.session(), None);
        assert_eq!(event.raw, None);
    }

    #[test]
    fn board_created_carries_the_actor_and_has_no_session() {
        let (board, project, name) = (BoardId::new(), ProjectId::new(), Name::new("ideas").unwrap());

        let event = Event::board_created(Source::fixture(), Actor::Human, board, project, name.clone());

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload.kind(), Kind::BoardCreated);
        assert_eq!(event.payload, Payload::BoardCreated { board, project, name });
        assert_eq!(event.payload.session(), None);
        assert_eq!(event.payload.map(), None);
        assert_eq!(event.raw, None);
    }

    #[test]
    fn node_added_carries_the_actor_and_has_no_session() {
        let (map, node) = (MapId::new(), NodeId::new());

        let properties = BTreeMap::from([("definition".to_string(), "D".to_string())]);

        let event = Event::node_added(Source::fixture(), Actor::Agent, map, node, "question", "Why?", properties.clone(), Vec::new());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload.kind(), Kind::NodeAdded);
        assert_eq!(event.payload, Payload::NodeAdded { map, node, kind: "question".into(), title: "Why?".into(), properties, references: Vec::new() });
        assert_eq!(event.payload.session(), None);
        assert_eq!(event.raw, None);
    }

    #[test]
    fn node_changed_carries_the_actor_and_the_full_new_state_and_has_no_session() {
        let (map, node) = (MapId::new(), NodeId::new());
        let properties = BTreeMap::from([("definition".to_string(), "D".to_string())]);

        let event = Event::node_changed(Source::fixture(), Actor::Human, map, node, "Why not?", properties.clone(), Vec::new());

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload.kind(), Kind::NodeChanged);
        assert_eq!(event.payload, Payload::NodeChanged { map, node, title: "Why not?".into(), properties, references: Vec::new() });
        assert_eq!(event.payload.session(), None);
        assert_eq!(event.raw, None);
    }

    #[test]
    fn edge_added_carries_the_actor_and_has_no_session() {
        let (map, from, to) = (MapId::new(), NodeId::new(), NodeId::new());

        let event = Event::edge_added(Source::fixture(), Actor::Agent, map, "answers", from, to);

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload.kind(), Kind::EdgeAdded);
        assert_eq!(event.payload, Payload::EdgeAdded { map, kind: "answers".into(), from, to });
        assert_eq!(event.payload.session(), None);
        assert_eq!(event.raw, None);
    }

    #[test]
    fn node_removed_carries_the_actor_and_has_no_session() {
        let (map, node) = (MapId::new(), NodeId::new());

        let event = Event::node_removed(Source::fixture(), Actor::Human, map, node);

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload.kind(), Kind::NodeRemoved);
        assert_eq!(event.payload, Payload::NodeRemoved { map, node });
        assert_eq!(event.payload.session(), None);
        assert_eq!(event.raw, None);
    }

    #[test]
    fn edge_removed_carries_the_actor_and_has_no_session() {
        let (map, from, to) = (MapId::new(), NodeId::new(), NodeId::new());

        let event = Event::edge_removed(Source::fixture(), Actor::Human, map, "answers", from, to);

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload.kind(), Kind::EdgeRemoved);
        assert_eq!(event.payload, Payload::EdgeRemoved { map, kind: "answers".into(), from, to });
        assert_eq!(event.payload.session(), None);
        assert_eq!(event.raw, None);
    }

    #[test]
    fn every_kind_is_found_by_its_name() {
        let kinds = [Kind::SessionCreated, Kind::SessionStarted, Kind::SessionStopped, Kind::Message, Kind::ToolUsed, Kind::SubagentStarted, Kind::SubagentReported, Kind::ProjectCreated, Kind::BoardCreated, Kind::MapCreated, Kind::NodeAdded, Kind::NodeChanged, Kind::NodeRemoved, Kind::EdgeAdded, Kind::EdgeRemoved];

        for kind in kinds {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(Kind::from_name("bogus"), None);
    }
}
