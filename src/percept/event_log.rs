use super::Event;

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
}
