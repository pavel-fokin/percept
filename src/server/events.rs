//! `GET /api/events` and `GET /api/events/{id}` - the project's log,
//! filtered by the same query the CLI's `percept events search`
//! builds, and one event by id. Scoped to `AppState.source.path`
//! always: `roots` is set by the handler, never by a caller.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::{EventLog, EventQuery};
use crate::shared::parse_time;
use crate::store;

/// `GET /api/events`'s query string, before it becomes an `EventQuery`.
/// Every field is optional; `kind` and `contains` are single values, not
/// repeated query keys, since a URL is typed once and comma-joined
/// rather than assembled from a form.
#[derive(Deserialize)]
pub struct Params {
    since: Option<String>,
    until: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    contains: Option<String>,
    size: Option<usize>,
    preview: Option<usize>,
}

/// How many of the most recent matches `GET /api/events` returns when
/// `size` is absent.
const DEFAULT_SIZE: usize = 200;

/// Why a request to this module's routes failed - the status a handler
/// answers with.
#[derive(Debug)]
pub enum Error {
    /// The query or the id was malformed - 400.
    Bad(String),
    /// No event in this project matches - 404.
    NotFound(String),
    /// The log couldn't be read - 500.
    Internal(String),
}

/// `GET /api/events`'s body: the matches in log order, and how many
/// matched before `size` cut them, so a page can say "eight of 1,240".
pub fn list(log: &dyn EventLog, params: Params, root: PathBuf) -> Result<Value, Error> {
    let (query, preview) = parse(params, root).map_err(Error::Bad)?;
    let events = log.load().map_err(|err| Error::Internal(err.to_string()))?;

    let matched: Vec<_> = events.into_iter().filter(|event| query.matches(event)).collect();
    let total = matched.len();
    let kept = take_recent(matched, query.size);

    let items: Vec<Value> = kept.iter().map(|event| store::summary(event, query.hit(event), preview)).collect();
    Ok(json!({ "events": items, "total": total }))
}

/// `GET /api/events/{id}`'s body: the whole wire event, no preview cut.
/// `Error::NotFound` when `id` names no event in this project;
/// `Error::Bad` when `id` doesn't parse.
pub fn get(log: &dyn EventLog, id: &str, root: &std::path::Path) -> Result<Value, Error> {
    let event_id = store::parse_event_id(id).map_err(|err| Error::Bad(err.to_string()))?;
    let event = log
        .get(event_id)
        .map_err(|err| Error::Internal(err.to_string()))?
        .filter(|event| event.source().path == root)
        .ok_or_else(|| Error::NotFound(format!("no event with id {id}")))?;
    Ok(json!({ "event": serde_json::to_value(store::Event::from(&event)).expect("store::Event always serializes") }))
}

/// `params` as an `EventQuery` scoped to `root`, and the `preview` size
/// a row's `content` is cut to - the same parsing `cli::parse_query`
/// does over `SearchArgs`, since both build the query `percept events
/// search` already defines.
fn parse(params: Params, root: PathBuf) -> Result<(EventQuery, usize), String> {
    let kinds = match params.kind.as_deref() {
        Some(s) if !s.is_empty() => s
            .split(',')
            .map(|kind| store::parse_kind(kind).map_err(|err| err.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };

    let since = params.since.as_deref().map(|s| moment("since", s)).transpose()?;
    let until = params.until.as_deref().map(|s| moment("until", s)).transpose()?;
    // An inverted window can never match - `since=1h&until=2h` is how
    // "between one and two hours ago" is mistyped, the same rule
    // `cli::parse_query` enforces.
    if let (Some(since), Some(until)) = (since, until) {
        if since >= until {
            return Err(format!("since {since} is not before until {until}"));
        }
    }

    let text = match params.contains {
        Some(term) if term.is_empty() => return Err("contains must not be blank".to_string()),
        Some(term) => vec![term],
        None => Vec::new(),
    };

    let size = match params.size {
        Some(0) => return Err("size must be at least 1".to_string()),
        Some(size) => size,
        None => DEFAULT_SIZE,
    };
    let preview = match params.preview {
        Some(0) => return Err("preview must be at least 1".to_string()),
        Some(preview) => preview,
        None => store::PREVIEW_CHARS,
    };

    Ok((
        EventQuery {
            since,
            until,
            roots: vec![root],
            kinds,
            text,
            size: Some(size),
            ..Default::default()
        },
        preview,
    ))
}

/// A `since`/`until` value, refused by the query parameter it came from -
/// no `--`, since a request has no flags.
fn moment(name: &str, s: &str) -> Result<crate::shared::Timestamp, String> {
    parse_time(s).ok_or_else(|| format!("invalid {name} value {s}"))
}

/// The `size` most recent of `matched`, kept in log order - the same
/// truncation `EventQuery::apply` does, run separately here so `list`
/// can still report `total` from before it.
fn take_recent(mut matched: Vec<crate::core::Event>, size: Option<usize>) -> Vec<crate::core::Event> {
    if let Some(size) = size {
        matched.drain(..matched.len().saturating_sub(size));
    }
    matched
}

#[cfg(test)]
mod tests;
