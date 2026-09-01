use crate::shared::{Id, Timestamp};

/// Identifies an Event.
pub type EventId = Id<Event>;

/// Who an Event is attributed to. Extend by adding a variant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    User,
    Model,
    // No producer yet - file and tool events come later. Part of the
    // actor vocabulary per ADR; the lint is suppressed rather than the
    // variant removed.
    #[allow(dead_code)]
    System,
}

/// Event-specific data. One variant per kind of fact the log records.
/// A variant carries typed fields only when the domain reads them -
/// `to_messages` needs `content`, so `MessageReceived` is typed.
/// `ToolUsed` stays opaque: nothing in the domain reads a tool call, so
/// `body` is the raw JSON text its source sent, unparsed.
#[derive(Clone)]
pub enum Payload {
    MessageReceived { content: String },
    ToolUsed { body: String },
}

/// What kind of fact an Event records - one variant per `Payload`
/// variant, as a value a caller can compare and filter on without
/// matching. The domain's word for what the wire calls `type`; `store`
/// owns the spelling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    MessageReceived,
    ToolUsed,
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
        }
    }
}
