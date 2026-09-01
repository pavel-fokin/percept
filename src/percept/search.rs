use super::{Actor, Event, EventKind};
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
    /// Keeps only the N most recent matches, still in log order.
    pub size: Option<usize>,
}

impl EventQuery {
    /// Whether `event` passes every filter this query sets.
    pub fn matches(&self, event: &Event) -> bool {
        self.since.is_none_or(|since| event.created_at() >= since)
            && self.until.is_none_or(|until| event.created_at() < until)
            && (self.actors.is_empty() || self.actors.contains(&event.actor()))
            && (self.sources.is_empty() || self.sources.iter().any(|s| s == event.source()))
            && (self.kinds.is_empty() || self.kinds.contains(&event.kind()))
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

/// Searches the committed log - domain-owned, the way `EventLog` is a
/// domain capability rather than an infrastructure detail.
/// `store::Jsonl` is today's implementation; the domain never depends
/// on it.
pub trait EventSearch: Send + Sync {
    /// Every event matching `query`, in log order.
    fn search(&self, query: &EventQuery) -> Result<Vec<Event>, Box<dyn std::error::Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{EventId, Payload};

    /// A message from `source`, timestamped `offset_minutes` back.
    fn event_at(source: &str, offset_minutes: i64) -> Event {
        Event::restore(
            EventId::new(),
            Actor::User,
            source.to_string(),
            None,
            Timestamp::now().minus_minutes(offset_minutes).unwrap(),
            Payload::MessageReceived {
                content: "hi".to_string(),
            },
        )
    }

    fn sources(events: &[Event]) -> Vec<String> {
        events.iter().map(|e| e.source().to_string()).collect()
    }

    #[test]
    fn a_default_query_keeps_everything_in_the_order_it_was_given() {
        let events = vec![event_at("a", 2), event_at("b", 1)];
        let kept = EventQuery::default().apply(events);
        assert_eq!(sources(&kept), vec!["a", "b"]);
    }

    #[test]
    fn size_keeps_the_most_recent_matches_but_preserves_log_order() {
        let events = vec![event_at("a", 3), event_at("b", 2), event_at("c", 1)];

        let kept = EventQuery {
            size: Some(2),
            ..Default::default()
        }
        .apply(events);

        assert_eq!(sources(&kept), vec!["b", "c"]);
    }

    #[test]
    fn size_larger_than_the_log_keeps_everything() {
        let events = vec![event_at("a", 2), event_at("b", 1)];

        let kept = EventQuery {
            size: Some(10),
            ..Default::default()
        }
        .apply(events);

        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn since_is_inclusive_and_until_is_exclusive() {
        let a = event_at("a", 30);
        let b = event_at("b", 20);
        let c = event_at("c", 10);

        let kept = EventQuery {
            since: Some(b.created_at()),
            until: Some(c.created_at()),
            ..Default::default()
        }
        .apply(vec![a, b, c]);

        assert_eq!(sources(&kept), vec!["b"]);
    }

    #[test]
    fn a_multi_valued_filter_matches_any_of_its_values() {
        let events = vec![event_at("a", 3), event_at("b", 2), event_at("c", 1)];

        let kept = EventQuery {
            sources: vec!["a".to_string(), "c".to_string()],
            ..Default::default()
        }
        .apply(events);

        assert_eq!(sources(&kept), vec!["a", "c"]);
    }

    #[test]
    fn filters_are_anded_together() {
        let mut wanted = event_at("a", 2);
        wanted = Event::restore(
            wanted.id(),
            Actor::Model,
            "a".to_string(),
            None,
            wanted.created_at(),
            Payload::MessageReceived {
                content: "hi".to_string(),
            },
        );

        let query = EventQuery {
            sources: vec!["a".to_string()],
            actors: vec![Actor::Model],
            ..Default::default()
        };

        assert!(query.matches(&wanted));
        assert!(!query.matches(&event_at("a", 1)));
    }
}
