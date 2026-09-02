use std::num::NonZeroUsize;
use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventQuery, EventSearch, Tool, ToolSpec};
use crate::shared::Timestamp;
use crate::store::{parse_actor, parse_kind, summarize, PREVIEW_CHARS};

/// The `search_events` tool: turns the model's JSON arguments into an
/// `EventQuery`, runs it, and returns each match as one summarized
/// JSONL line - constant size per event, per the primitive rule in
/// AGENTS.md.
pub struct SearchEvents {
    log: Arc<dyn EventSearch>,
}

impl SearchEvents {
    pub fn new(log: Arc<dyn EventSearch>) -> Self {
        Self { log }
    }
}

const NAME: &str = "search_events";

/// Cap on matches when the caller names no `size`, so an unfiltered
/// search can't return - or replay next turn - the whole log.
const DEFAULT_SIZE: usize = 20;

const DESCRIPTION: &str = "Search the percept event log. Every field is \
    an optional filter; omit a field to leave it off. A multi-valued \
    filter matches any of its values. Timestamps are ISO-8601. Results \
    come back oldest first, one JSON object per line, with long strings \
    cut short. Without `size` the newest 20 matches come back; raise it \
    deliberately when you need more.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "since": {"type": "string", "description": "ISO-8601 lower bound, inclusive"},
    "until": {"type": "string", "description": "ISO-8601 upper bound, exclusive"},
    "actors": {"type": "array", "items": {"type": "string", "enum": ["user", "model", "system"]}},
    "sources": {"type": "array", "items": {"type": "string"}, "description": "the writer that produced the event, e.g. tui or claude-code"},
    "kinds": {"type": "array", "items": {"type": "string", "enum": ["message.received", "thought.recorded", "tool.called", "tool.resulted"]}},
    "contains": {"type": "array", "items": {"type": "string", "minLength": 1}, "description": "a substring, case-insensitive, that one of the event's payload strings must carry; any of the values matches"},
    "size": {"type": "integer", "description": "keep only the N most recent matches"},
    "preview": {"type": "integer", "minimum": 1, "description": "how many characters of content each line keeps, cut around the first `contains` hit when there is one; default 120"}
  },
  "additionalProperties": false
}"#;

/// Unknown keys are refused rather than ignored, so a misspelt filter
/// comes back as an error the model can read, not as a wider search.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Args {
    since: Option<String>,
    until: Option<String>,
    actors: Vec<String>,
    sources: Vec<String>,
    kinds: Vec<String>,
    contains: Vec<String>,
    size: Option<usize>,
    /// Non-zero by type, so serde refuses `0` where the schema says
    /// `minimum: 1`.
    preview: Option<NonZeroUsize>,
}

impl Tool for SearchEvents {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<String, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;

        let since = args.since.as_deref().map(parse_time).transpose()?;
        let until = args.until.as_deref().map(parse_time).transpose()?;
        let actors = args
            .actors
            .iter()
            .map(|a| parse_actor(a))
            .collect::<Result<_, _>>()?;
        let kinds = args
            .kinds
            .iter()
            .map(|k| parse_kind(k))
            .collect::<Result<_, _>>()?;

        // The schema says `minLength: 1`, but serde won't enforce it - a
        // blank term would quietly match everything.
        if let Some(term) = args.contains.iter().find(|t| t.trim().is_empty()) {
            return Err(format!("contains term {term:?} must not be blank").into());
        }
        let preview = args.preview.map_or(PREVIEW_CHARS, NonZeroUsize::get);

        let query = EventQuery {
            since,
            until,
            actors,
            sources: args.sources,
            kinds,
            text: args.contains,
            size: args.size.or(Some(DEFAULT_SIZE)),
        };

        let events = self.log.search(&query)?;
        Ok(events
            .iter()
            .map(|event| summarize(event, query.hit(event), preview))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

/// ISO-8601 only - the model is told the current time and works out
/// absolute bounds itself, so no relative shorthand and no clock here.
fn parse_time(s: &str) -> Result<Timestamp, Box<dyn std::error::Error>> {
    s.parse()
        .map_err(|_| format!("invalid timestamp {s:?}, expected ISO-8601").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Event, EventId, Payload};
    use std::sync::Mutex;

    /// The filters a test asserts `run` translated correctly.
    #[derive(Default)]
    struct Seen {
        actors: Vec<Actor>,
        text: Vec<String>,
        size: Option<usize>,
    }

    #[derive(Default)]
    struct FakeSearch {
        events: Vec<Event>,
        seen: Mutex<Seen>,
    }

    impl EventSearch for FakeSearch {
        fn search(&self, query: &EventQuery) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
            *self.seen.lock().unwrap() = Seen {
                actors: query.actors.clone(),
                text: query.text.clone(),
                size: query.size,
            };
            Ok(query.apply(self.events.clone()))
        }
    }

    fn message(source: &str, content: &str) -> Event {
        Event::restore(
            EventId::new(),
            Actor::User,
            source.to_string(),
            None,
            Timestamp::now(),
            Payload::MessageReceived {
                content: content.to_string(),
            },
        )
    }

    fn tool() -> SearchEvents {
        SearchEvents::new(Arc::new(FakeSearch {
            events: vec![message("tui", "hello"), message("claude-code", "world")],
            ..Default::default()
        }))
    }

    #[test]
    fn spec_names_the_tool_and_carries_valid_schema_json() {
        let spec = tool().spec();
        assert_eq!(spec.name, "search_events");
        let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn run_returns_one_summarized_line_per_match() {
        let out = tool().run(r#"{"sources":["tui"]}"#).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1);
        let line: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(line["source"], "tui");
        assert_eq!(line["payload"]["content"], "hello");
    }

    #[test]
    fn run_translates_string_filters_into_domain_enums() {
        let search = Arc::new(FakeSearch {
            events: vec![message("tui", "hi")],
            ..Default::default()
        });
        let tool = SearchEvents::new(search.clone());

        tool.run(r#"{"actors":["user"],"contains":["deploy"],"size":3}"#)
            .unwrap();

        let seen = search.seen.lock().unwrap();
        assert!(seen.actors == vec![Actor::User]);
        assert_eq!(seen.text, vec!["deploy".to_string()]);
        assert_eq!(seen.size, Some(3));
    }

    #[test]
    fn an_empty_object_searches_with_only_the_default_size() {
        let search = Arc::new(FakeSearch {
            events: vec![message("tui", "a"), message("tui", "b")],
            ..Default::default()
        });
        let out = SearchEvents::new(search.clone()).run("{}").unwrap();

        assert_eq!(out.lines().count(), 2);
        assert_eq!(search.seen.lock().unwrap().size, Some(DEFAULT_SIZE));
    }

    #[test]
    fn malformed_arguments_are_an_error() {
        assert!(tool().run("not json").is_err());
    }

    #[test]
    fn an_unknown_actor_is_an_error() {
        assert!(tool().run(r#"{"actors":["robot"]}"#).is_err());
    }

    #[test]
    fn a_non_iso_timestamp_is_an_error() {
        assert!(tool().run(r#"{"since":"yesterday"}"#).is_err());
    }

    #[test]
    fn preview_sizes_the_content_window_and_zero_is_an_error() {
        let out = tool().run(r#"{"preview":2}"#).unwrap();
        let line: serde_json::Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
        assert_eq!(line["payload"]["content"], "he\u{2026}");
        assert_eq!(line["preview"]["len"], 5);
        assert!(tool().run(r#"{"preview":0}"#).is_err());
    }

    #[test]
    fn an_unknown_argument_is_an_error() {
        assert!(tool().run(r#"{"limit":3}"#).is_err());
    }

    #[test]
    fn a_blank_contains_term_is_an_error() {
        assert!(tool().run(r#"{"contains":[""]}"#).is_err());
    }

    #[test]
    fn run_keeps_only_events_whose_payload_carries_the_term() {
        let out = tool().run(r#"{"contains":["orl"]}"#).unwrap();

        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1);
        let line: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(line["source"], "claude-code");
    }
}
