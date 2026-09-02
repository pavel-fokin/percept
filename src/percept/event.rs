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
/// and feeds `ToolResulted` back, so both are. `ToolUsed` stays
/// opaque: it arrives as JSON from another writer and nothing in the
/// domain reads it, so `body` is that raw text, unparsed.
#[derive(Clone)]
pub enum Payload {
    MessageReceived {
        content: String,
    },
    ToolUsed {
        body: String,
    },
    ThoughtRecorded {
        content: String,
    },
    /// A tool call percept's own loop made. `arguments` is JSON text
    /// the domain routes by `tool` name but never parses - the tool
    /// owns that shape. `call_id` ties this to its `ToolResulted`.
    ToolCalled {
        call_id: String,
        tool: String,
        arguments: String,
    },
    /// What percept fed back for the `ToolCalled` with the same
    /// `call_id`: the tool's output, or its error text.
    ToolResulted {
        call_id: String,
        content: String,
    },
}

/// What kind of fact an Event records - one variant per `Payload`
/// variant, as a value a caller can compare and filter on without
/// matching. The domain's word for what the wire calls `type`; `store`
/// owns the spelling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    MessageReceived,
    ToolUsed,
    ThoughtRecorded,
    ToolCalled,
    ToolResulted,
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
        call_id: String,
        tool: String,
        arguments: String,
        source: String,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            Actor::Model,
            source,
            causation_id,
            Payload::ToolCalled {
                call_id,
                tool,
                arguments,
            },
        )
    }

    /// A `tool.resulted` event - always percept feeding a tool's output
    /// back, never the model.
    pub fn tool_resulted(
        call_id: String,
        content: String,
        source: String,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            Actor::System,
            source,
            causation_id,
            Payload::ToolResulted { call_id, content },
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
            Payload::ToolUsed { .. } => EventKind::ToolUsed,
            Payload::ThoughtRecorded { .. } => EventKind::ThoughtRecorded,
            Payload::ToolCalled { .. } => EventKind::ToolCalled,
            Payload::ToolResulted { .. } => EventKind::ToolResulted,
        }
    }
}
