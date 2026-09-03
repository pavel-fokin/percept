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
    /// `arguments` - the envelope is not searched, since `actor` and
    /// `source` already have filters. A blank term is contained by
    /// everything, so a boundary that can receive one rejects it before
    /// building a query.
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
            && (self.sources.is_empty() || self.sources.iter().any(|s| s == event.source()))
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

    /// A user message from `tui` carrying `content`, for the
    /// text-filter tests.
    fn message(content: &str) -> Event {
        Event::restore(
            EventId::new(),
            Actor::User,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::MessageReceived {
                content: content.to_string(),
            },
        )
    }

    fn contents(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .map(|e| match e.payload() {
                Payload::MessageReceived { content } => content.clone(),
                _ => panic!("not a message"),
            })
            .collect()
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

    #[test]
    fn a_text_term_matches_a_substring_case_insensitively() {
        let events = vec![message("Deploy the API"), message("hello")];

        let kept = EventQuery {
            text: vec!["deploy".to_string()],
            ..Default::default()
        }
        .apply(events);

        assert_eq!(contents(&kept), vec!["Deploy the API"]);
    }

    #[test]
    fn a_hit_is_the_earliest_offset_of_any_term_in_content() {
        let event = message("Ship it, then DEPLOY it, then ship again");
        let query = EventQuery {
            text: vec!["deploy".to_string(), "then".to_string()],
            ..Default::default()
        };
        assert_eq!(query.hit(&event), Some(9..13));

        let off = EventQuery::default();
        assert_eq!(off.hit(&event), None);
    }

    #[test]
    fn a_hit_counts_characters_of_the_original_text() {
        // `İ` lowercases to two characters; an offset taken on the
        // lowercased copy would land one past the term.
        let event = message("İİ deploy");
        let query = EventQuery {
            text: vec!["deploy".to_string()],
            ..Default::default()
        };
        assert_eq!(query.hit(&event), Some(3..9));
    }

    #[test]
    fn a_hit_spans_the_original_characters_of_a_term_that_expands() {
        let event = message("say İ now");
        let query = EventQuery {
            text: vec!["İ".to_string()],
            ..Default::default()
        };
        assert_eq!(query.hit(&event), Some(4..5));
    }

    #[test]
    fn a_final_sigma_matches_and_hits_alike() {
        let event = message("ΟΔΥΣΣΕΥΣ went home");
        let query = EventQuery {
            text: vec!["ΟΔΥΣΣΕΥΣ".to_string()],
            ..Default::default()
        };
        assert!(query.matches(&event));
        assert_eq!(query.hit(&event), Some(0..8));
    }

    #[test]
    fn an_empty_term_matches_empty_content_without_panicking() {
        let event = message("");
        let query = EventQuery {
            text: vec![String::new()],
            ..Default::default()
        };
        assert!(query.matches(&event));
        assert_eq!(query.hit(&event), Some(0..0));
    }

    #[test]
    fn a_tool_call_matching_on_its_tool_name_has_no_hit() {
        let call = Event::restore(
            EventId::new(),
            Actor::Model,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::ToolCalled {
                tool: "search_events".to_string(),
                arguments: "{}".to_string(),
            },
        );
        let query = EventQuery {
            text: vec!["search".to_string()],
            ..Default::default()
        };
        assert!(query.matches(&call));
        assert_eq!(query.hit(&call), None);
    }

    #[test]
    fn a_text_term_matches_every_payload_kind() {
        let payloads = vec![
            Payload::MessageReceived {
                content: "deploy it".to_string(),
            },
            Payload::ThoughtRecorded {
                content: "deploy it".to_string(),
            },
            Payload::ToolResulted {
                content: "deploy it".to_string(),
            },
            Payload::ToolCalled {
                tool: "deploy_tool".to_string(),
                arguments: "{}".to_string(),
            },
        ];
        let query = EventQuery {
            text: vec!["deploy".to_string()],
            ..Default::default()
        };

        for payload in payloads {
            let event = Event::restore(
                EventId::new(),
                Actor::User,
                "tui".to_string(),
                None,
                Timestamp::now(),
                payload,
            );
            assert!(query.matches(&event));
        }
    }

    #[test]
    fn a_tool_call_matches_by_tool_name_or_by_arguments() {
        let call = Event::restore(
            EventId::new(),
            Actor::Model,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::ToolCalled {
                tool: "search_events".to_string(),
                arguments: r#"{"kinds":["tool.called"]}"#.to_string(),
            },
        );

        let by_tool = EventQuery {
            text: vec!["search".to_string()],
            ..Default::default()
        };
        let by_arguments = EventQuery {
            text: vec!["tool.called".to_string()],
            ..Default::default()
        };

        assert!(by_tool.matches(&call));
        assert!(by_arguments.matches(&call));
    }

    #[test]
    fn a_text_term_does_not_match_the_envelope() {
        // The source is "claude-code"; the payload only says "hi".
        let event = event_at("claude-code", 1);

        let query = EventQuery {
            text: vec!["claude".to_string()],
            ..Default::default()
        };

        assert!(!query.matches(&event));
    }

    #[test]
    fn text_terms_match_any_of_their_values() {
        let events = vec![
            message("deploy the api"),
            message("ship it"),
            message("hello"),
        ];

        let kept = EventQuery {
            text: vec!["deploy".to_string(), "SHIP".to_string()],
            ..Default::default()
        }
        .apply(events);

        assert_eq!(contents(&kept), vec!["deploy the api", "ship it"]);
    }

    #[test]
    fn a_blank_term_matches_everything_as_documented() {
        let events = vec![message("a"), message("b")];

        let kept = EventQuery {
            text: vec![String::new()],
            ..Default::default()
        }
        .apply(events);

        assert_eq!(kept.len(), 2);
    }
}
