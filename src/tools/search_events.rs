use std::num::NonZeroUsize;
use std::sync::Arc;

use serde::Deserialize;

use crate::core::{EventQuery, EventSearch};
use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::store::{optional_time, parse_actor, parse_kind, summarize, PREVIEW_CHARS};

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
    "since": {"type": "string", "description": "lower bound, inclusive: ISO-8601, or 1d/2h/30m back from now"},
    "until": {"type": "string", "description": "upper bound, exclusive: ISO-8601, or 1d/2h/30m back from now"},
    "actors": {"type": "array", "items": {"type": "string", "enum": ["user", "model", "system"]}},
    "sources": {"type": "array", "items": {"type": "string"}, "description": "the writer that produced the event, e.g. percept-code or claude-code"},
    "kinds": {"type": "array", "items": {"type": "string", "enum": ["message.received", "thought.recorded", "tool.called", "tool.resulted", "node.added", "node.removed", "edge.added", "edge.removed", "model.called", "session.started", "file.cited"]}},
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

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;

        let since = optional_time(args.since.as_deref())?;
        let until = optional_time(args.until.as_deref())?;
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
        Ok(ToolOutput::text(
            events
                .iter()
                .map(|event| summarize(event, query.hit(event), preview))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }
}

#[cfg(test)]
mod tests;
