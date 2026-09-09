use super::{Event, EventId};

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
    /// event it builds - all under the same lock `append` takes, so a
    /// second writer's own `compute` can never run between this one's
    /// load and its append. The seam a mint that must stay unique
    /// closes through: a `NodeAdded` payload's `seq` is counted from
    /// the events `compute` is given, so two writers racing to mint the
    /// next number for one kind can never agree - the second always
    /// sees the first's event already committed.
    fn append_computed(
        &self,
        compute: Box<dyn FnOnce(Vec<Event>) -> Result<Event, Box<dyn std::error::Error>> + '_>,
    ) -> Result<Event, Box<dyn std::error::Error>>;

    /// As `append_computed`, but `compute` builds a whole batch: every
    /// event it returns is appended in order, under the one lock hold
    /// that also covers the load `compute` was handed - so a writer
    /// that must check and mint several events together, a document's
    /// worth of nodes and edges, sees them all committed or none, and
    /// no other writer's own append can land in between.
    fn append_batch_computed(
        &self,
        compute: Box<dyn FnOnce(Vec<Event>) -> Result<Vec<Event>, Box<dyn std::error::Error>> + '_>,
    ) -> Result<Vec<Event>, Box<dyn std::error::Error>>;
}
