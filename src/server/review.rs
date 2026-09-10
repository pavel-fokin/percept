//! The queue `GET /api/review` serves: per map, the claims a reader has
//! not yet reviewed, grouped by the map's settlement question. Folded
//! fresh on every call from the log `cut` is handed - nothing here is
//! cached, so a write between two requests is seen on the next one.
//! `ReviewResponse` and the structs it nests mirror `web/src/types.ts`
//! one to one, so the page reads the same shape this module writes.

use serde::Serialize;

use crate::core::{
    Event, EventId, EventLog, HumanId, Map, MapError, Node, NodeId, Schemas, Scope, Settlement,
    Source, Standing,
};
use crate::mapstore::{self, JudgeError};
use crate::shared::Timestamp;

mod sources;
use sources::{EventIndex, SourceEntry};

/// `GET /api/review`'s response: one entry per schema with at least one
/// headline kind, each folded from `log.load()` fresh. `next` is the
/// same lines `judged_since_block` builds for the session-start hook,
/// cut to what was judged since the project's latest `session.started`
/// event from any source - the page has no client of its own to filter
/// by - or `None` when nothing was.
#[derive(Serialize)]
pub struct ReviewResponse {
    pub maps: Vec<MapQueue>,
    pub next: Option<String>,
}

/// One map's queue: its claims, grouped by the question or task each
/// answers.
#[derive(Serialize)]
pub struct MapQueue {
    pub name: String,
    pub purpose: String,
    pub since: Option<String>,
    pub groups: Vec<Group>,
}

/// Claims that share a settlement question, or a task with none.
#[derive(Serialize)]
pub struct Group {
    pub heading: Option<Heading>,
    pub claims: Vec<Row>,
}

/// The question or task a group's rows answer.
#[derive(Serialize)]
pub struct Heading {
    pub id: String,
    pub title: String,
    pub raised_at: String,
}

/// An alternative weighed and lost, folded under the row that answers
/// the same question.
#[derive(Serialize)]
pub struct OptionRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub why: Option<String>,
    pub standing: Option<String>,
    pub dispute: Option<String>,
    pub sources: Vec<SourceEntry>,
}

/// One claim in the queue: a headline node the map's fold marks
/// `claimed`, or judged since the map was last finished.
#[derive(Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub base: OptionRow,
    pub added_at: String,
    pub was: Option<NodeRef>,
    pub reopens: Vec<NodeRef>,
    pub options: Vec<OptionRow>,
}

/// A node this one supersedes, or one it reopens - just enough to link
/// back to it: its short id and name.
#[derive(Serialize)]
pub struct NodeRef {
    pub id: String,
    pub name: String,
}

pub fn cut(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
) -> Result<ReviewResponse, Box<dyn std::error::Error>> {
    let events = log.load()?;
    let scope = source.scope();
    let maps = schemas.fold_all(&scope, &events)?;
    let index = EventIndex::new(&events);
    let map_queues: Vec<MapQueue> = maps
        .iter()
        .filter(|map| !map.schema().headline_kinds.is_empty())
        .map(|map| map_queue(map, &index))
        .collect();
    let next = last_session(&events, &scope).and_then(|at| mapstore::judged_since_block(&maps, at));
    Ok(ReviewResponse { maps: map_queues, next })
}

/// The `since` every client's next `session.started` block will use, so
/// a judgment the foot names here is one the model is guaranteed to
/// see next: for each client - grouped by source name and path - the
/// latest `session.started` in `scope`, then the earliest of those
/// maxes across clients, since that is the oldest `since` any client's
/// next start will compute. `None` when `scope` holds no events at all;
/// when it holds events but no `session.started`, the earliest event's
/// `created_at` stands in, so a judgment made before any hook ran still
/// shows.
fn last_session(events: &[Event], scope: &Scope) -> Option<Timestamp> {
    let latest_by_client = mapstore::latest_session_per_client(events, scope);
    if latest_by_client.is_empty() {
        return events
            .iter()
            .filter(|event| scope.admits(event))
            .map(|event| event.created_at())
            .min();
    }
    latest_by_client.into_values().min()
}

/// What a `/api/dispute`, `/api/confirm`, or `/api/finish` write is
/// refused for: the HTTP status it earns - 404 for a node id no map
/// holds, 400 for anything else a write path refuses.
#[derive(Debug)]
pub enum Refused {
    Bad(String),
    NotFound(String),
}

impl From<JudgeError> for Refused {
    fn from(err: JudgeError) -> Self {
        let not_found = matches!(
            &err,
            JudgeError::Map(MapError::NoSuchNode { .. } | MapError::UnknownShortId(_))
        );
        if not_found {
            Self::NotFound(err.to_string())
        } else {
            Self::Bad(err.to_string())
        }
    }
}

/// `POST /api/dispute`: appends a `claim.disputed` naming `node` on
/// `map`, with `why` - refused with `Bad` when `why` is blank.
pub fn dispute(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
    map: &str,
    node: &str,
    why: String,
) -> Result<EventId, Refused> {
    Ok(mapstore::dispute(log, schemas, source, me, map, node, why)?.id())
}

/// `POST /api/confirm`: appends a `claim.confirmed` naming `node` on
/// `map`.
pub fn confirm(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
    map: &str,
    node: &str,
) -> Result<EventId, Refused> {
    Ok(mapstore::confirm(log, schemas, source, me, map, node)?.id())
}

/// `POST /api/finish`: appends one `review.finished` naming every id in
/// `nodes` that still resolves against `map` - a short id or
/// `kind:name`, as `maps confirm` accepts - skipping one the human
/// wrote themselves, the same as one that no longer resolves: a node
/// removed since the page loaded must not block finishing until a
/// reload. Also names the heading of every group one of those rows
/// sits in, so the page need not send heading ids of its own.
pub fn finish(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
    map: &str,
    nodes: &[String],
) -> Result<EventId, Refused> {
    let folded =
        mapstore::fold_map(log, schemas, map, &source.scope()).map_err(|err| Refused::Bad(err.to_string()))?;
    let mut ids: Vec<NodeId> = Vec::new();
    for node in nodes {
        if let Ok(id) = mapstore::resolve_judged_node(&folded, node) {
            ids.push(id);
        }
    }
    if let Some(settlement) = &folded.schema().settlement {
        let claims: Vec<&Node> = folded.headlines().filter(|node| is_claim(&folded, node)).collect();
        for group in grouped(&folded, settlement, &claims) {
            let Some(of_node) = group.of_node else { continue };
            let already_in = group.rows.iter().any(|row| ids.contains(&row.id));
            if already_in && !ids.contains(&of_node.id) {
                ids.push(of_node.id);
            }
        }
    }
    let event = Event::review_finished(map.to_string(), ids, me, source.clone(), None);
    log.append(&event).map_err(|err| Refused::Bad(err.to_string()))?;
    Ok(event.id())
}

/// One map's queue: its `since`, and its claims grouped by question.
fn map_queue(map: &Map, index: &EventIndex) -> MapQueue {
    let schema = map.schema();
    let claims: Vec<&Node> = map.headlines().filter(|node| is_claim(map, node)).collect();
    let groups = match &schema.settlement {
        Some(settlement) => grouped(map, settlement, &claims),
        None => vec![RawGroup { of_node: None, rows: claims }],
    };
    MapQueue {
        name: schema.name.clone(),
        purpose: schema.purpose.clone(),
        since: map.last_finished().map(|at| at.to_string()),
        groups: groups.iter().map(|group| group_json(map, group, index)).collect(),
    }
}

/// Whether `node` belongs in the cut: a model-written node whose
/// standing is `Claimed`, or whose latest judgment landed at or after
/// the map's last finish. A user-written node's standing is `None`, so
/// it is never a claim. No finish yet means every judgment still
/// counts.
fn is_claim(map: &Map, node: &Node) -> bool {
    match map.standing(node.id) {
        Some(Standing::Claimed) => true,
        Some(_) => match map.last_finished() {
            None => true,
            Some(finished) => map.judged_at(node.id).is_some_and(|at| at >= finished),
        },
        None => false,
    }
}

/// One heading a queue's rows sit under: `of_node` is the question or
/// task the rows answer, `None` for a map with no settlement or for the
/// rows a `by` claim resolves nothing settles.
struct RawGroup<'a> {
    of_node: Option<&'a Node>,
    rows: Vec<&'a Node>,
}

/// `claims` grouped by `settlement`: for every `of` node in the map,
/// the decision that `Map::settled_by` says settles it now, when that
/// decision is a `by` claim in the cut - so a decision recorded as a
/// correction, with a `supersedes` edge and no `resolves` edge of its
/// own, still lands under the question its predecessor answered. An
/// `of` claim nothing in the cut settles is its own group, and a `by`
/// claim that settles no `of` node in the map goes to a last group with
/// no heading. A group whose `of` node reopens a decision sorts first;
/// otherwise by the `of` node's `added_at`, ascending.
fn grouped<'a>(map: &'a Map, settlement: &Settlement, claims: &[&'a Node]) -> Vec<RawGroup<'a>> {
    let mut resolved: Vec<(&'a Node, Vec<&'a Node>)> = Vec::new();
    let mut settled_ids: Vec<NodeId> = Vec::new();
    for of_node in map.nodes().iter().filter(|node| node.kind == settlement.of) {
        let rows: Vec<&'a Node> = map
            .settled_by(of_node.id)
            .into_iter()
            .filter_map(|decision| claims.iter().copied().find(|claim| claim.id == decision.id))
            .collect();
        if !rows.is_empty() {
            settled_ids.extend(rows.iter().map(|node| node.id));
            resolved.push((of_node, rows));
        }
    }

    let grouped_of_ids: Vec<NodeId> = resolved.iter().map(|(of_node, _)| of_node.id).collect();
    let mut pairs: Vec<(&'a Node, Vec<&'a Node>)> = resolved;
    for &node in claims.iter().filter(|node| node.kind == settlement.of) {
        if !grouped_of_ids.contains(&node.id) {
            pairs.push((node, vec![node]));
        }
    }

    // A group whose question reopens a decision sorts first, so
    // `!reopens` (false before true) beats `added_at` as the key.
    pairs.sort_by_cached_key(|(of_node, _)| (map.reopens(of_node.id).is_empty(), of_node.added_at));

    let mut groups: Vec<RawGroup<'a>> = pairs
        .into_iter()
        .map(|(of_node, rows)| RawGroup { of_node: Some(of_node), rows })
        .collect();

    let orphaned: Vec<&'a Node> = claims
        .iter()
        .copied()
        .filter(|node| node.kind == settlement.by && !settled_ids.contains(&node.id))
        .collect();
    if !orphaned.is_empty() {
        groups.push(RawGroup { of_node: None, rows: orphaned });
    }
    groups
}

/// One group as JSON: its heading, `None` for the orphan group and for
/// a group whose only row is its own question, and its rows by
/// `added_at` ascending.
fn group_json(map: &Map, group: &RawGroup, index: &EventIndex) -> Group {
    let is_self_group = |of_node: &Node| group.rows.len() == 1 && group.rows[0].id == of_node.id;
    let heading = group.of_node.filter(|node| !is_self_group(node)).map(|node| Heading {
        id: map.short_id(node.id).unwrap_or_default(),
        title: node.name.clone(),
        raised_at: node.added_at.to_string(),
    });
    let mut rows = group.rows.clone();
    rows.sort_by_key(|node| node.added_at);
    let options: Vec<&Node> = group.of_node.map(|node| map.weighed_for(node.id)).unwrap_or_default();
    Group {
        heading,
        claims: rows.iter().map(|node| row_json(map, node, &options, index)).collect(),
    }
}

/// A node's short id and name - `None` when it has neither.
fn node_ref(map: &Map, node: &Node) -> NodeRef {
    NodeRef {
        id: map.short_id(node.id).unwrap_or_default(),
        name: node.name.clone(),
    }
}

/// One row: the claim itself, what it replaced and reopens, and the
/// alternatives weighed against the group's question, computed once per
/// group and handed to every row in it.
fn row_json(map: &Map, node: &Node, options: &[&Node], index: &EventIndex) -> Row {
    let was = map.predecessors(node.id).first().map(|was| node_ref(map, was));
    let reopens = map.reopens(node.id).into_iter().map(|decision| node_ref(map, decision)).collect();
    Row {
        base: option_json(map, node, index),
        added_at: node.added_at.to_string(),
        was,
        reopens,
        options: options.iter().map(|option| option_json(map, option, index)).collect(),
    }
}

/// One alternative under a row's `options`, or the claim row itself.
fn option_json(map: &Map, node: &Node, index: &EventIndex) -> OptionRow {
    OptionRow {
        id: map.short_id(node.id).unwrap_or_default(),
        kind: node.kind.clone(),
        name: node.name.clone(),
        why: node.properties.get("why").cloned(),
        standing: map.standing(node.id).map(|standing| standing.to_string()),
        dispute: map.dispute(node.id).map(str::to_string),
        sources: index.sources_json(&node.sources),
    }
}

#[cfg(test)]
mod tests;
