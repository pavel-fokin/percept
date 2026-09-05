use std::ops::Range;

use super::{Actor, Event, EventKind, Payload};
use crate::shared::Timestamp;

/// A query over the committed log. Every field is a filter; `None`, or
/// an empty `Vec`, means that filter is off, so a default query matches
/// everything. Holds only absolute timestamps and never reads a clock -
/// resolving a relative shorthand like `1d` against `now` is the CLI's
/// job, before the domain sees it.
#[derive(Default)]
pub struct EventQuery {
    /// Inclusive lower bound.
    pub since: Option<Timestamp>,
    /// Exclusive upper bound - so adjacent windows tile with no overlap
    /// and no gap.
    pub until: Option<Timestamp>,
    pub actors: Vec<Actor>,
    pub sources: Vec<String>,
    pub kinds: Vec<EventKind>,
    /// A term matches when one of the event's payload strings carries
    /// it as a substring, case-insensitively; an event passes when any
    /// term does. Payload strings are `content`, `tool`, and
    /// `arguments`, on a map change `map`, `kind`, `name`, `reason`, and
    /// property values, and on a `model.called` event its `model` name.
    /// The envelope is not searched, since `actor` and `source` already
    /// have filters. A blank term is contained by everything, so a
    /// boundary that can receive one rejects it before building a
    /// query.
    pub text: Vec<String>,
    /// Keeps only the N most recent matches, still in log order.
    pub size: Option<usize>,
}

impl EventQuery {
    /// Whether `event` passes every filter this query sets.
    pub fn matches(&self, event: &Event) -> bool {
        self.since.is_none_or(|since| event.created_at() >= since)
            && self.until.is_none_or(|until| event.created_at() < until)
            && (self.actors.is_empty() || self.actors.contains(&event.actor()))
            && (self.sources.is_empty() || self.sources.iter().any(|s| s == &event.source().name))
            && (self.kinds.is_empty() || self.kinds.contains(&event.kind()))
            && (self.text.is_empty() || self.text.iter().any(|term| carries(event.payload(), term)))
    }

    /// The matches among `events`, which arrive in log order. `size`
    /// slices the tail once every other filter has run, rather than
    /// sorting - the order it keeps is the one it was given.
    pub fn apply(&self, mut events: Vec<Event>) -> Vec<Event> {
        events.retain(|event| self.matches(event));
        if let Some(size) = self.size {
            events.drain(..events.len().saturating_sub(size));
        }
        events
    }
}

impl EventQuery {
    /// Where in `event`'s `content` the text filter hits: the character
    /// range of the earliest occurrence of any term, by the same rule
    /// `matches` applies. `None` when the filter is off, the event has
    /// no `content`, or no term is in it - a `tool.called` event can
    /// match on `tool` or `arguments` and still have no hit here.
    pub fn hit(&self, event: &Event) -> Option<Range<usize>> {
        if self.text.is_empty() {
            return None;
        }
        let content = event.payload().content()?;
        // Lowercasing can turn one character into several, so each
        // lowercased character remembers which original it came from
        // and the range is counted on the original text.
        let mut lower = String::with_capacity(content.len());
        let mut origin = Vec::with_capacity(content.len());
        for (i, c) in content.chars().enumerate() {
            for l in c.to_lowercase() {
                lower.push(l);
                origin.push(i);
            }
        }
        let (at, term) = self
            .text
            .iter()
            .map(fold)
            .filter_map(|term| lower.find(&term).map(|at| (at, term)))
            .min_by_key(|(at, _)| *at)?;
        let first = lower[..at].chars().count();
        let last = first + term.chars().count();
        let start = origin.get(first).copied().unwrap_or(0);
        let end = origin.get(last.saturating_sub(1)).map_or(start, |&o| o + 1);
        Some(start..end)
    }
}

/// Whether one of `payload`'s strings carries `term` as a
/// case-insensitive substring.
fn carries(payload: &Payload, term: &str) -> bool {
    let term = fold(term);
    let has = |s: &str| fold(s).contains(&term);
    match payload {
        Payload::MessageReceived { content }
        | Payload::ThoughtRecorded { content }
        | Payload::ToolResulted { content } => has(content),
        Payload::ToolCalled { tool, arguments } => has(tool) || has(arguments),
        Payload::NodeAdded {
            map,
            kind,
            name,
            properties,
            ..
        } => has(map) || has(kind) || has(name) || properties.values().any(|v| has(v)),
        Payload::NodeRemoved { map, reason, .. } => has(map) || has(reason),
        Payload::EdgeAdded { map, kind, .. } | Payload::EdgeRemoved { map, kind, .. } => {
            has(map) || has(kind)
        }
        Payload::ModelCalled(usage) => has(&usage.model),
    }
}

/// Lowercases character by character - the same mapping `hit` walks -
/// rather than `str::to_lowercase`, whose context-sensitive cases (a
/// Greek final sigma) would make `matches` and `hit` disagree.
fn fold(s: impl AsRef<str>) -> String {
    s.as_ref().chars().flat_map(char::to_lowercase).collect()
}

/// Searches the committed log - domain-owned, the way `EventLog` is a
/// domain capability rather than an infrastructure detail.
/// `store::Jsonl` is today's implementation; the domain never depends
/// on it.
pub trait EventSearch: Send + Sync {
    /// Every event matching `query`, in log order.
    fn search(&self, query: &EventQuery) -> Result<Vec<Event>, Box<dyn std::error::Error>>;
}

#[cfg(test)]
mod tests;
