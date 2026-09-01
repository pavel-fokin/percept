use super::{Actor, Event, EventKind};
use crate::shared::Timestamp;

/// A query over the committed log. Every field is a filter; `None`, or
/// an empty `Vec`, means that filter is off. Holds only absolute
/// timestamps and never reads a clock - resolving a relative shorthand
/// like `1d` against `now` is the CLI's job, before the domain sees it.
#[derive(Default)]
pub struct EventQuery {
    /// Inclusive lower bound.
    pub since: Option<Timestamp>,
    /// Exclusive upper bound - so adjacent windows tile with no overlap
    /// and no gap.
    pub until: Option<Timestamp>,
    /// Matches any of these actors.
    pub actors: Vec<Actor>,
    /// Matches any of these sources.
    pub sources: Vec<String>,
    /// Matches any of these kinds.
    pub kinds: Vec<EventKind>,
    /// Keeps only the N most recent matches, still in log order.
    pub size: Option<usize>,
}

/// Searches the committed log - domain-owned, the way `EventLog` is a
/// domain capability rather than an infrastructure detail.
/// `store::Jsonl` is today's implementation; the domain never depends
/// on it.
pub trait EventSearch: Send + Sync {
    /// Every event matching `query`, in log order. `size` slices the
    /// tail after every other filter runs, rather than sorting - a
    /// caller passes events already in commit order.
    fn search(&self, query: &EventQuery) -> Result<Vec<Event>, Box<dyn std::error::Error>>;
}
