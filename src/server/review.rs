//! The queue `GET /api/review` serves: per map, the claims a reader has
//! not yet reviewed, grouped by the map's settlement question. Folded
//! fresh on every call from the log `cut` is handed - nothing here is
//! cached, so a write between two requests is seen on the next one.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::core::{
    Actor, Event, EventId, EventLog, HumanId, Map, Node, NodeId, Payload, Schemas, Scope,
    Settlement, Source, Standing,
};
use crate::mapstore;
use crate::shared::Timestamp;

mod sources;
use sources::EventIndex;

/// The edge kind a correction points at what it replaces over -
/// `core::Map` enforces this by string too; `review` reads the same
/// name rather than re-deriving it.
const SUPERSEDES: &str = "supersedes";

/// The response `GET /api/review` serves: `{"maps": [...], "next":
/// ...}`, one map entry per schema with at least one headline kind,
/// each folded from `log.load()` fresh - the log is never cached
/// between calls. `next` is the same lines `judged_since_block` builds
/// for the session-start hook, cut to what was judged since the
/// project's latest `session.started` event from any source - the page
/// has no client of its own to filter by - or `null` when nothing was.
pub fn cut(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
) -> Result<Value, Box<dyn std::error::Error>> {
    let events = log.load()?;
    let scope = source.scope();
    let maps = schemas.fold_all(&scope, &events)?;
    let index = EventIndex::new(&events);
    let map_values: Vec<Value> = maps
        .iter()
        .filter(|map| !map.schema().headline_kinds.is_empty())
        .map(|map| map_json(map, &scope, &events, &index))
        .collect();
    let next = last_session(&events, &scope).and_then(|at| mapstore::judged_since_block(&maps, at));
    Ok(json!({ "maps": map_values, "next": next }))
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
    let admitted: Vec<&Event> = events.iter().filter(|event| scope.admits(event)).collect();
    let mut latest_by_client: HashMap<(String, PathBuf), Timestamp> = HashMap::new();
    for event in admitted.iter().filter(|event| matches!(event.payload(), Payload::SessionStarted)) {
        let key = (event.source().name.clone(), event.source().path.clone());
        let at = event.created_at();
        latest_by_client
            .entry(key)
            .and_modify(|current| *current = (*current).max(at))
            .or_insert(at);
    }
    if latest_by_client.is_empty() {
        return admitted.iter().map(|event| event.created_at()).min();
    }
    latest_by_client.into_values().min()
}

/// What a `/api/dispute`, `/api/confirm`, or `/api/finish` write
/// yields: the appended event's id, or an error with the HTTP status
/// it earns - 404 for a node id no map holds, 400 for anything else a
/// write path refuses.
pub enum ApiOutcome {
    Ok(EventId),
    Bad(String),
    NotFound(String),
}

fn outcome_of(err: Box<dyn std::error::Error>) -> ApiOutcome {
    if mapstore::is_unknown_node(err.as_ref()) {
        ApiOutcome::NotFound(err.to_string())
    } else {
        ApiOutcome::Bad(err.to_string())
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
) -> ApiOutcome {
    if why.trim().is_empty() {
        return ApiOutcome::Bad("why must not be blank".to_string());
    }
    match mapstore::judge(log, schemas, source, map, node, |map, node, source| {
        Event::claim_disputed(map, node, why, me, source, None)
    }) {
        Ok(event) => ApiOutcome::Ok(event.id()),
        Err(err) => outcome_of(err),
    }
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
) -> ApiOutcome {
    match mapstore::judge(log, schemas, source, map, node, |map, node, source| {
        Event::claim_confirmed(map, node, me, source, None)
    }) {
        Ok(event) => ApiOutcome::Ok(event.id()),
        Err(err) => outcome_of(err),
    }
}

/// `POST /api/finish`: appends one `review.finished` naming every id in
/// `nodes` that still resolves against `map` - a short id or
/// `kind:name`, as `maps confirm` accepts - skipping one the human
/// wrote themselves, the same as one that no longer resolves: a node
/// removed since the page loaded must not block finishing until a
/// reload, and the page may send a group's heading along with its
/// claims.
pub fn finish(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
    map: &str,
    nodes: &[String],
) -> ApiOutcome {
    let folded = match mapstore::fold_map(log, schemas, map, &source.scope()) {
        Ok(folded) => folded,
        Err(err) => return outcome_of(err),
    };
    let mut ids = Vec::new();
    for node in nodes {
        let Ok(id) = folded.resolve_str(node) else {
            continue;
        };
        let resolved = folded.node(id).expect("resolve_str returns a live node's id");
        if !matches!(resolved.actor, Actor::Human(_)) {
            ids.push(id);
        }
    }
    let event = Event::review_finished(map.to_string(), ids, me, source.clone(), None);
    match log.append(&event) {
        Ok(()) => ApiOutcome::Ok(event.id()),
        Err(err) => outcome_of(err),
    }
}

/// One map's queue: its `since`, and its claims grouped by question.
fn map_json(map: &Map, scope: &Scope, events: &[Event], index: &EventIndex) -> Value {
    let schema = map.schema();
    let since = last_finished(events, scope, &schema.name);
    let claims: Vec<&Node> = map
        .headlines()
        .filter(|node| is_claim(map, node, since))
        .collect();
    let groups = match &schema.settlement {
        Some(settlement) => grouped(map, settlement, &claims),
        None => vec![Group {
            of_node: None,
            rows: claims,
        }],
    };
    json!({
        "name": schema.name,
        "purpose": schema.purpose,
        "since": since.map(|at| at.to_string()),
        "groups": groups.iter().map(|group| group_json(map, group, index)).collect::<Vec<_>>(),
    })
}

/// The `created_at` of the latest `review.finished` event naming
/// `map_name`, within `scope` - the way `cli::hook::last_session` finds
/// the latest `session.started`. `None` when this map has never been
/// finished.
fn last_finished(events: &[Event], scope: &Scope, map_name: &str) -> Option<Timestamp> {
    events
        .iter()
        .filter(|event| scope.admits(event))
        .filter_map(|event| match event.payload() {
            Payload::ReviewFinished { map, .. } if map == map_name => Some(event.created_at()),
            _ => None,
        })
        .max()
}

/// Whether `node` belongs in the cut: a model-written node whose
/// standing is `Claimed`, or whose latest judgment landed at or after
/// `since`. A user-written node's standing is `None`, so it is never a
/// claim. `since` absent means this map has never been finished, so no
/// node of it carries `Seen` - every judgment still counts.
fn is_claim(map: &Map, node: &Node, since: Option<Timestamp>) -> bool {
    match map.standing(node.id) {
        Some(Standing::Claimed) => true,
        Some(_) => match since {
            None => true,
            Some(at) => map.judged_since(at).any(|(judged, _, _)| judged.id == node.id),
        },
        None => false,
    }
}

/// One heading a queue's rows sit under: `of_node` is the question or
/// task the rows answer, `None` for a map with no settlement or for the
/// rows a `by` claim resolves nothing settles.
struct Group<'a> {
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
fn grouped<'a>(map: &'a Map, settlement: &Settlement, claims: &[&'a Node]) -> Vec<Group<'a>> {
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
    let mut groups: Vec<Group<'a>> = resolved
        .into_iter()
        .map(|(of_node, rows)| Group { of_node: Some(of_node), rows })
        .collect();
    for &node in claims.iter().filter(|node| node.kind == settlement.of) {
        if !grouped_of_ids.contains(&node.id) {
            groups.push(Group {
                of_node: Some(node),
                rows: vec![node],
            });
        }
    }

    groups.sort_by(|a, b| {
        let a_node = a.of_node.expect("only the orphan group, appended below, has none");
        let b_node = b.of_node.expect("only the orphan group, appended below, has none");
        let a_reopens = !map.reopens(a_node.id).is_empty();
        let b_reopens = !map.reopens(b_node.id).is_empty();
        b_reopens
            .cmp(&a_reopens)
            .then(a_node.added_at.cmp(&b_node.added_at))
    });

    let orphaned: Vec<&'a Node> = claims
        .iter()
        .copied()
        .filter(|node| node.kind == settlement.by && !settled_ids.contains(&node.id))
        .collect();
    if !orphaned.is_empty() {
        groups.push(Group {
            of_node: None,
            rows: orphaned,
        });
    }
    groups
}

/// The node `from`'s `supersedes` edge points at, if it has one.
fn supersedes_target(map: &Map, from: NodeId) -> Option<NodeId> {
    map.edges()
        .iter()
        .find(|edge| edge.kind == SUPERSEDES && edge.from == from)
        .map(|edge| edge.to)
}

/// One group as JSON: its heading, and its rows by `added_at` ascending.
fn group_json(map: &Map, group: &Group, index: &EventIndex) -> Value {
    let (id, title, raised_at) = match group.of_node {
        Some(node) => (
            map.short_id(node.id).unwrap_or_default(),
            node.name.clone(),
            json!(node.added_at.to_string()),
        ),
        None => (String::new(), String::new(), Value::Null),
    };
    let mut rows = group.rows.clone();
    rows.sort_by_key(|node| node.added_at);
    let question = group.of_node.map(|node| node.id);
    json!({
        "id": id,
        "title": title,
        "raised_at": raised_at,
        "claims": rows.iter().map(|node| row_json(map, node, question, index)).collect::<Vec<_>>(),
    })
}

/// One row: the claim itself, what it replaced and reopens, and the
/// alternatives weighed against `question` when the row's group has
/// one.
fn row_json(map: &Map, node: &Node, question: Option<NodeId>, index: &EventIndex) -> Value {
    let was = supersedes_target(map, node.id).and_then(|id| map.node(id)).map(|was| {
        json!({ "id": map.short_id(was.id).unwrap_or_default(), "name": was.name })
    });
    let reopens: Vec<Value> = map
        .reopens(node.id)
        .into_iter()
        .map(|decision| json!({ "id": map.short_id(decision.id).unwrap_or_default(), "name": decision.name }))
        .collect();
    let options: Vec<Value> = question
        .map(|question| {
            map.weighed_for(question)
                .into_iter()
                .map(|option| option_json(map, option, index))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": map.short_id(node.id).unwrap_or_default(),
        "kind": node.kind,
        "name": node.name,
        "why": node.properties.get("why"),
        "standing": map.standing(node.id).map(|standing| standing.to_string()),
        "dispute": map.dispute(node.id),
        "added_at": node.added_at.to_string(),
        "was": was,
        "reopens": reopens,
        "options": options,
        "sources": index.sources_json(&node.sources),
    })
}

/// One alternative under a row's `options`.
fn option_json(map: &Map, node: &Node, index: &EventIndex) -> Value {
    json!({
        "id": map.short_id(node.id).unwrap_or_default(),
        "kind": node.kind,
        "name": node.name,
        "why": node.properties.get("why"),
        "standing": map.standing(node.id).map(|standing| standing.to_string()),
        "dispute": map.dispute(node.id),
        "sources": index.sources_json(&node.sources),
    })
}

#[cfg(test)]
mod tests;
