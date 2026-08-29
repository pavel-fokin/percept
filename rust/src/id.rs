use uuid::Uuid;

/// Uniquely identifies an Event. Per ADR, entity IDs use UUIDv7 - time
/// ordered, so IDs sort by creation - wrapped in a type dedicated to that
/// entity, so IDs from different entities can't be swapped by mistake.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EventId(Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}
