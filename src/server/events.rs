//! `GET /api/events` and `GET /api/events/{id}` - the web view's own cut
//! of the log, not a general query: it folds a `tool.resulted` into the
//! `tool.called` that caused it, so it is never a row, whatever the
//! filter asks for. `percept events search` is the general query over
//! the same log; a caller here asking for `type=tool.resulted` gets no
//! rows, and that is this route's rule, not a bug. Scoped to
//! `AppState.source.path` always: `roots` is set by the handler, never
//! by a caller.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::{Event, EventKind, EventLog, EventQuery};
use crate::shared::parse_time;
use crate::store;

/// Event kinds this view folds into another row rather than showing as
/// one of its own. Adding a second folded kind is one line here.
const FOLDED_KINDS: &[EventKind] = &[EventKind::ToolResulted];

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
    actor: Option<String>,
    contains: Option<String>,
    size: Option<usize>,
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

/// `GET /api/events`'s body: the matches in log order, how many matched
/// before `size` cut them, and each match's `carried` answer - the
/// `FOLDED_KINDS` event, if any, that names it as its cause. A folded
/// event never appears among the matches themselves, whatever the
/// filter asks for.
pub fn list(log: &dyn EventLog, params: Params, root: PathBuf) -> Result<Value, Error> {
    let (query, text_query, preview, size) = parse(params, root.clone()).map_err(Error::Bad)?;
    let events = log.load().map_err(|err| Error::Internal(err.to_string()))?;

    // A result is presented inside the call that caused it. Searching
    // that result therefore finds the visible call, while the other
    // filters still describe the call itself.
    let matching_answers: std::collections::HashSet<_> = events
        .iter()
        .filter(|event| FOLDED_KINDS.contains(&event.kind()) && text_query.matches(event))
        .filter_map(Event::causation_id)
        .collect();

    let mut matched: Vec<&Event> = events
        .iter()
        .filter(|event| {
            !FOLDED_KINDS.contains(&event.kind())
                && query.matches(event)
                && (text_query.text.is_empty() || text_query.matches(event) || matching_answers.contains(&event.id()))
        })
        .collect();
    let total = matched.len();
    matched.drain(..total.saturating_sub(size));
    let kept = matched;

    let kept_ids: std::collections::HashSet<_> = kept.iter().map(|event| event.id()).collect();
    let carried: Vec<_> = events
        .iter()
        .filter(|event| {
            FOLDED_KINDS.contains(&event.kind()) && event.causation_id().is_some_and(|cause| kept_ids.contains(&cause))
        })
        .collect();

    let items: Vec<Value> = kept.iter().map(|event| store::summary(event, text_query.hit(event), preview)).collect();
    let carried_items: Vec<Value> = carried
        .iter()
        .map(|event| store::summary(event, text_query.hit(event), preview))
        .collect();
    Ok(json!({
        "events": items,
        "carried": carried_items,
        "total": total,
        "project": root.to_string_lossy(),
    }))
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
fn parse(params: Params, root: PathBuf) -> Result<(EventQuery, EventQuery, usize, usize), String> {
    let kinds = match params.kind.as_deref() {
        Some(s) if !s.is_empty() => s
            .split(',')
            .map(|kind| store::parse_kind(kind).map_err(|err| err.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };

    // `None` for `me`: `EventQuery::matches` compares actors by name, so
    // `Actor::Human(None)` already matches every human, whoever they are.
    let actors = match params.actor.as_deref() {
        Some(s) if !s.is_empty() => s
            .split(',')
            .map(|actor| store::parse_actor(actor, None).map_err(|err| err.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };

    let since = params.since.as_deref().map(|s| moment("since", s)).transpose()?;
    let until = params.until.as_deref().map(|s| moment("until", s)).transpose()?;
    let window = EventQuery { since, until, ..Default::default() };
    if let Some((since, until)) = window.inverted_window() {
        return Err(format!("since {since} is not before until {until}"));
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
    let text_query = EventQuery {
        roots: vec![root.clone()],
        text,
        ..Default::default()
    };
    Ok((
        EventQuery {
            since,
            until,
            actors,
            roots: vec![root],
            kinds,
            ..Default::default()
        },
        text_query,
        store::PREVIEW_CHARS,
        size,
    ))
}

/// A `since`/`until` value, refused by the query parameter it came from -
/// no `--`, since a request has no flags.
fn moment(name: &str, s: &str) -> Result<crate::shared::Timestamp, String> {
    parse_time(s).ok_or_else(|| format!("invalid {name} value {s}"))
}

#[cfg(test)]
mod tests;
