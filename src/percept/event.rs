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
#[derive(Clone)]
pub enum Payload {
    MessageReceived { content: String },
}

/// One recorded fact in the conversation log. Append-only: a committed
/// Event never changes. `actor` and the `payload` variant together say
/// what happened; `causation_id` says what led to it. `Clone` copies a
/// record, id and all - it never makes a committed event editable.
#[derive(Clone)]
pub struct Event {
    id: EventId,
    seq: u64,
    actor: Actor,
    causation_id: Option<EventId>,
    created_at: Timestamp,
    payload: Payload,
}

impl Event {
    /// A `message.received` event. Fills `id` and `created_at`; the
    /// caller owns `seq` (the log's ordering) and `causation_id`.
    pub fn message_received(
        actor: Actor,
        content: String,
        seq: u64,
        causation_id: Option<EventId>,
    ) -> Self {
        Self {
            id: EventId::new(),
            seq,
            actor,
            causation_id,
            created_at: Timestamp::now(),
            payload: Payload::MessageReceived { content },
        }
    }

    /// Rebuilds an Event from stored fields - the persistence boundary,
    /// where `id` and `created_at` come from storage rather than being
    /// minted fresh.
    pub fn restore(
        id: EventId,
        seq: u64,
        actor: Actor,
        causation_id: Option<EventId>,
        created_at: Timestamp,
        payload: Payload,
    ) -> Self {
        Self {
            id,
            seq,
            actor,
            causation_id,
            created_at,
            payload,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn actor(&self) -> Actor {
        self.actor
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
}
