//! The queue `GET /api/review` serves: per map, the claims a reader has
//! not yet reviewed, grouped by the map's settlement question. Folded
//! fresh on every call from the log `cut` is handed - nothing here is
//! cached, so a write between two requests is seen on the next one.

use serde_json::{json, Value};

use crate::core::{
    Actor, Event, EventId, EventLog, HumanId, Map, Node, NodeId, Payload, Schemas, Scope,
    Settlement, Source, Standing,
};
use crate::mapstore;
use crate::shared::Timestamp;

mod sources;
use sources::EventIndex;

/// The edge kind names the built-in schemas fix a settlement to: a
/// `by` node resolves its `of` node over `RESOLVES`, an option answers
/// its question over `ANSWERS`, and a correction points at what it
/// replaces over `SUPERSEDES`. `core::Map` enforces these by string
/// too; `review` reads the same names rather than re-deriving them,
/// since a `Settlement` names the two node kinds, not the edge.
const RESOLVES: &str = "resolves";
const ANSWERS: &str = "answers";
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

/// The `created_at` of the latest `session.started` event in `scope`,
/// from any source - unlike `cli::hook::last_session`, which filters to
/// one client, the review page is opened from a browser, not a coding
/// client, so every client's last session here counts.
fn last_session(events: &[Event], scope: &Scope) -> Option<Timestamp> {
    events
        .iter()
        .filter(|event| scope.admits(event))
        .filter(|event| matches!(event.payload(), Payload::SessionStarted))
        .map(Event::created_at)
        .max()
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
/// `nodes`, resolved against `map` - a short id or `kind:name`, as
/// `maps confirm` accepts - skipping one the human wrote themselves,
/// since the page may send a group's heading along with its claims.
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
        let id = match folded.resolve_str(node) {
            Ok(id) => id,
            Err(err) => return outcome_of(err.into()),
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

/// `claims` grouped by `settlement`: a `by` claim under the `of` node
/// its `resolves` edge names, an `of` claim nothing in the cut resolves
/// as its own group, and a `by` claim that resolves nothing in a last
/// group with no heading. A group whose `of` node reopens a decision
/// sorts first; otherwise by the `of` node's `added_at`, ascending.
fn grouped<'a>(map: &'a Map, settlement: &Settlement, claims: &[&'a Node]) -> Vec<Group<'a>> {
    let mut resolved: Vec<(NodeId, Vec<&'a Node>)> = Vec::new();
    let mut orphaned: Vec<&'a Node> = Vec::new();
    for &node in claims.iter().filter(|node| node.kind == settlement.by) {
        match resolves_target(map, node.id) {
            Some(question) => match resolved.iter_mut().find(|(id, _)| *id == question) {
                Some((_, rows)) => rows.push(node),
                None => resolved.push((question, vec![node])),
            },
            None => orphaned.push(node),
        }
    }

    let resolved_ids: Vec<NodeId> = resolved.iter().map(|(id, _)| *id).collect();
    let mut groups: Vec<Group<'a>> = resolved
        .into_iter()
        .filter_map(|(id, rows)| map.node(id).map(|of_node| Group { of_node: Some(of_node), rows }))
        .collect();
    for &node in claims.iter().filter(|node| node.kind == settlement.of) {
        if !resolved_ids.contains(&node.id) {
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

    if !orphaned.is_empty() {
        groups.push(Group {
            of_node: None,
            rows: orphaned,
        });
    }
    groups
}

/// The node `from`'s `resolves` edge points at, if it has one.
fn resolves_target(map: &Map, from: NodeId) -> Option<NodeId> {
    map.edges()
        .iter()
        .find(|edge| edge.kind == RESOLVES && edge.from == from)
        .map(|edge| edge.to)
}

/// The node `from`'s `supersedes` edge points at, if it has one.
fn supersedes_target(map: &Map, from: NodeId) -> Option<NodeId> {
    map.edges()
        .iter()
        .find(|edge| edge.kind == SUPERSEDES && edge.from == from)
        .map(|edge| edge.to)
}

/// Every model-written node with an `answers` edge to `question`, any
/// standing, in `added_at` order - a group's row lists these as its
/// weighed-and-lost alternatives.
fn answers(map: &Map, question: NodeId) -> Vec<&Node> {
    let mut nodes: Vec<&Node> = map
        .edges()
        .iter()
        .filter(|edge| edge.kind == ANSWERS && edge.to == question)
        .filter_map(|edge| map.node(edge.from))
        .filter(|node| !matches!(node.actor, Actor::Human(_)))
        .collect();
    nodes.sort_by_key(|node| node.added_at);
    nodes
}

/// One group as JSON: its heading, and its rows by `added_at` ascending.
fn group_json(map: &Map, group: &Group, index: &EventIndex) -> Value {
    let (id, title, raised_at) = match group.of_node {
        Some(node) => (
            map.short_id(node.id).unwrap_or_default(),
            node.name.clone(),
            node.added_at.to_string(),
        ),
        None => (String::new(), String::new(), String::new()),
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
            answers(map, question)
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
