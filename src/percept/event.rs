use std::collections::BTreeMap;

use super::NodeId;
use crate::shared::{Id, Timestamp};

/// Identifies an Event.
pub type EventId = Id<Event>;

/// Who an Event is attributed to. Extend by adding a variant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    User,
    Model,
    /// percept itself acting - so far, feeding a tool's output back as
    /// `tool.resulted`.
    System,
}

/// Event-specific data. One variant per kind of fact the log records.
/// A variant carries typed fields only when the domain produces or
/// reads them - `to_messages` needs `content`, so `MessageReceived` is
/// typed; `App` assembles a thought from streamed text, so
/// `ThoughtRecorded` is; `App` runs the loop that emits `ToolCalled`
/// and feeds `ToolResulted` back, so both are. A `ToolCalled` from
/// another writer has no paired result in this log, but replays as
/// context all the same.
#[derive(Clone, Debug)]
pub enum Payload {
    MessageReceived {
        content: String,
    },
    ThoughtRecorded {
        content: String,
    },
    /// A tool call. `arguments` is JSON text the domain routes by
    /// `tool` name but never parses - the tool owns that shape. From
    /// percept's own loop, its `ToolResulted` names this event as
    /// cause; from another writer, no result follows in this log.
    ToolCalled {
        tool: String,
        arguments: String,
    },
    /// What percept fed back for a `ToolCalled` - its output or its
    /// error text. `causation_id` points at the call.
    ToolResulted {
        content: String,
    },
    /// A node added to a cognitive map. `sources` names the events the
    /// node was folded from.
    NodeAdded {
        map: String,
        node: NodeId,
        kind: String,
        name: String,
        properties: BTreeMap<String, String>,
        sources: Vec<EventId>,
    },
    /// A node removed from a cognitive map, with why.
    NodeRemoved {
        map: String,
        node: NodeId,
        reason: String,
        sources: Vec<EventId>,
    },
    /// An edge added to a cognitive map. Carries no id of its own -
    /// `kind`, `from`, and `to` identify one.
    EdgeAdded {
        map: String,
        kind: String,
        from: NodeId,
        to: NodeId,
        sources: Vec<EventId>,
    },
    /// An edge removed from a cognitive map.
    EdgeRemoved {
        map: String,
        kind: String,
        from: NodeId,
        to: NodeId,
        sources: Vec<EventId>,
    },
}

impl Payload {
    /// The event's text - the one string that runs long, and the one a
    /// reader wants to see more of. A tool call carries none: its
    /// `tool` and `arguments` are the model's own short strings. Nor
    /// does a map change: its fields are all short.
    pub fn content(&self) -> Option<&str> {
        match self {
            Self::MessageReceived { content }
            | Self::ThoughtRecorded { content }
            | Self::ToolResulted { content } => Some(content),
            Self::ToolCalled { .. }
            | Self::NodeAdded { .. }
            | Self::NodeRemoved { .. }
            | Self::EdgeAdded { .. }
            | Self::EdgeRemoved { .. } => None,
        }
    }
}

/// What kind of fact an Event records - one variant per `Payload`
/// variant, as a value a caller can compare and filter on without
/// matching. The domain's word for what the wire calls `type`; `store`
/// owns the spelling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    MessageReceived,
    ThoughtRecorded,
    ToolCalled,
    ToolResulted,
    NodeAdded,
    NodeRemoved,
    EdgeAdded,
    EdgeRemoved,
}

/// One recorded fact in the conversation log. Append-only: a committed
/// Event never changes. `actor` and the `payload` variant together say
/// what happened; `source` says which writer produced it; `causation_id`
/// says what led to it. `Clone` copies a record, id and all - it never
/// makes a committed event editable.
#[derive(Clone)]
pub struct Event {
    id: EventId,
    actor: Actor,
    source: String,
    causation_id: Option<EventId>,
    created_at: Timestamp,
    payload: Payload,
}

impl Event {
    /// A fresh event. `id` and `created_at` are minted here, so no
    /// caller outside the domain decides what an event's identity is;
    /// the caller owns `source` (which writer produced it) and
    /// `causation_id`.
    pub fn new(
        actor: Actor,
        source: String,
        causation_id: Option<EventId>,
        payload: Payload,
    ) -> Self {
        Self {
            id: EventId::new(),
            actor,
            source,
            causation_id,
            created_at: Timestamp::now(),
            payload,
        }
    }

    /// A `message.received` event.
    pub fn message_received(
        actor: Actor,
        content: String,
        source: String,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            actor,
            source,
            causation_id,
            Payload::MessageReceived { content },
        )
    }

    /// A `thought.recorded` event.
    pub fn thought_recorded(
        actor: Actor,
        content: String,
        source: String,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            actor,
            source,
            causation_id,
            Payload::ThoughtRecorded { content },
        )
    }

    /// A `tool.called` event - always the model's action.
    pub fn tool_called(
        tool: String,
        arguments: String,
        source: String,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            Actor::Model,
            source,
            causation_id,
            Payload::ToolCalled { tool, arguments },
        )
    }

    /// A `tool.resulted` event - always percept feeding a tool's output
    /// back, never the model.
    pub fn tool_resulted(content: String, source: String, causation_id: Option<EventId>) -> Self {
        Self::new(
            Actor::System,
            source,
            causation_id,
            Payload::ToolResulted { content },
        )
    }

    /// Rebuilds an Event from stored fields - the persistence boundary,
    /// where `id` and `created_at` come from storage rather than being
    /// minted fresh.
    pub fn restore(
        id: EventId,
        actor: Actor,
        source: String,
        causation_id: Option<EventId>,
        created_at: Timestamp,
        payload: Payload,
    ) -> Self {
        Self {
            id,
            actor,
            source,
            causation_id,
            created_at,
            payload,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn actor(&self) -> Actor {
        self.actor
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn causation_id(&self) -> Option<EventId> {
        self.causation_id
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    pub fn kind(&self) -> EventKind {
        match self.payload {
            Payload::MessageReceived { .. } => EventKind::MessageReceived,
            Payload::ThoughtRecorded { .. } => EventKind::ThoughtRecorded,
            Payload::ToolCalled { .. } => EventKind::ToolCalled,
            Payload::ToolResulted { .. } => EventKind::ToolResulted,
            Payload::NodeAdded { .. } => EventKind::NodeAdded,
            Payload::NodeRemoved { .. } => EventKind::NodeRemoved,
            Payload::EdgeAdded { .. } => EventKind::EdgeAdded,
            Payload::EdgeRemoved { .. } => EventKind::EdgeRemoved,
        }
    }
}
