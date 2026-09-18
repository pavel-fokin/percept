use super::{Event, EventId};

/// `compute` for `append_batch_computed`: builds a whole batch of
/// events from the events loaded under the same lock the append takes.
pub(crate) type ComputeEvents<'a> =
    Box<dyn FnOnce(Vec<Event>) -> Result<Vec<Event>, Box<dyn std::error::Error>> + 'a>;

/// Persists the append-only log so the transcript survives a restart -
/// domain-owned, the way `Model` is a domain capability rather than an
/// infrastructure detail. `store::Jsonl` is today's implementation; the
/// domain never depends on it.
pub trait EventLog: Send + Sync {
    /// Appends one committed event. An error here must reach the
    /// caller before the event is treated as part of the transcript -
    /// losing a committed event silently is worse than failing loudly.
    fn append(&self, event: &Event) -> Result<(), Box<dyn std::error::Error>>;

    /// Loads every event in the log, in commit order.
    fn load(&self) -> Result<Vec<Event>, Box<dyn std::error::Error>>;

    /// An id the log doesn't carry is an absence, not an error - the
    /// caller decides what to make of it.
    fn get(&self, id: EventId) -> Result<Option<Event>, Box<dyn std::error::Error>>;

    /// Loads every event, hands them to `compute`, and appends the
    /// batch it builds in order. The load, computation, and append use
    /// one lock hold, so the batch commits whole or not at all and no
    /// other computed append can land between its events.
    fn append_batch_computed(
        &self,
        compute: ComputeEvents<'_>,
    ) -> Result<Vec<Event>, Box<dyn std::error::Error>>;
}
