//! The queue `GET /api/review` serves: per map, the headline nodes
//! changed since the review last opened, grouped by the headline node
//! each row's edges point at. Folded fresh on every call from the log
//! `cut` is handed - nothing here is cached, so a write between two
//! requests is seen on the next one. `ReviewResponse` and the structs
//! it nests mirror `web/src/types.ts` one to one, so the page reads the
//! same shape this module writes.

use std::collections::HashMap;

use serde::Serialize;

use crate::core::{Actor, Dir, EventId, EventLog, HumanId, Map, MapError, Node, NodeId, Schemas, Source};
use crate::mapstore;
use crate::shared::Timestamp;

mod sources;
use sources::{EventIndex, SourceEntry};

/// `GET /api/review`'s response: one entry per schema with at least one
/// headline kind, each folded from `log.load()` fresh.
#[derive(Serialize)]
pub struct ReviewResponse {
    pub maps: Vec<MapQueue>,
}

/// One map's queue: its claims, grouped by the headline node each row's
/// edges point at.
#[derive(Serialize)]
pub struct MapQueue {
    pub name: String,
    pub purpose: String,
    pub since: Option<String>,
    pub groups: Vec<Group>,
}

/// Claims that share a heading, or a row with none.
#[derive(Serialize)]
pub struct Group {
    pub heading: Option<Heading>,
    pub claims: Vec<Row>,
}

/// The headline node a group's rows point at.
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
    pub changed_by: &'static str,
    pub changed_why: Option<String>,
    pub changed_at: String,
    pub sources: Vec<SourceEntry>,
}

/// One claim in the queue: a headline node changed since the review
/// last opened.
#[derive(Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub base: OptionRow,
    pub added_at: String,
    pub edges: Vec<EdgeRef>,
    pub related: Vec<OptionRow>,
}

/// One edge touching a row, named by kind and direction, and the node
/// on its other end.
#[derive(Serialize)]
pub struct EdgeRef {
    pub kind: String,
    pub dir: &'static str,
    pub node: NodeRef,
}

/// A node reached by an edge - just enough to link back to it: its
/// short id and name.
#[derive(Serialize)]
pub struct NodeRef {
    pub id: String,
    pub name: String,
}

/// The queue as of now: every headline node whose `changed_at` is at or
/// after `since`, of any actor - every node when `since` is `None`.
pub fn cut(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    since: Option<Timestamp>,
) -> Result<ReviewResponse, Box<dyn std::error::Error>> {
    let events = log.load()?;
    let scope = source.scope();
    let maps = schemas.fold_all(&scope, &events)?;
    let index = EventIndex::new(&events);
    let map_queues: Vec<MapQueue> = maps
        .iter()
        .filter(|map| !map.schema().headline_kinds.is_empty())
        .map(|map| map_queue(map, since, &index))
        .collect();
    Ok(ReviewResponse { maps: map_queues })
}

/// What a `/api/change` write is refused for: the HTTP status it
/// earns - 404 for a node id no map holds, 400 for anything else a
/// write path refuses.
#[derive(Debug)]
pub enum Refused {
    Bad(String),
    NotFound(String),
}

/// `POST /api/change`: appends a `node.changed` naming `node` on `map`,
/// carrying only `why` - refused with `Bad` when `why` is blank, with
/// `NotFound` when `node` resolves against no node in `map`.
pub fn change(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
    map: &str,
    node: &str,
    why: String,
) -> Result<EventId, Refused> {
    if why.trim().is_empty() {
        return Err(Refused::Bad("why must not be blank".to_string()));
    }
    let scope = source.scope();
    let folded =
        mapstore::fold_map(log, schemas, map, &scope).map_err(|err| Refused::Bad(err.to_string()))?;
    let node_id = mapstore::NodeRefArgs::ShortId(node.to_string())
        .resolve(&folded)
        .map_err(|err| classify(&err))?;
    let target = folded.node(node_id).expect("resolve returns a live node's id");
    let node_ref = crate::core::NodeRef {
        kind: target.kind.clone(),
        name: target.name.clone(),
    };
    let event = mapstore::commit(
        log,
        schemas,
        map,
        &scope,
        source,
        &[],
        crate::core::Actor::Human(me),
        move |sources| crate::core::Mutation::ChangeNode {
            node: node_ref,
            name: None,
            properties: Default::default(),
            sources,
            why: Some(why),
        },
    )
    .map_err(|err| Refused::Bad(err.to_string()))?;
    Ok(event.id())
}

/// `Bad` or `NotFound`, from the `MapError` a write refused with.
fn classify(error: &MapError) -> Refused {
    let not_found = matches!(
        error,
        MapError::NoSuchNode { .. } | MapError::UnknownShortId(_)
    );
    if not_found {
        Refused::NotFound(error.to_string())
    } else {
        Refused::Bad(error.to_string())
    }
}

/// One map's queue: its `since`, and its claims grouped by heading.
fn map_queue(map: &Map, since: Option<Timestamp>, index: &EventIndex) -> MapQueue {
    let schema = map.schema();
    let claims: Vec<&Node> = map
        .headlines()
        .filter(|node| since.is_none_or(|since| node.changed_at >= since))
        // The human's own writes - a landmark they added, a Wrong they
        // gave - are not theirs to review.
        .filter(|node| !matches!(node.changed_by, Actor::Human(_)))
        .collect();
    let groups = grouped(map, &claims);
    MapQueue {
        name: schema.name.clone(),
        purpose: schema.purpose.clone(),
        since: since.map(|at| at.to_string()),
        groups: groups.iter().map(|group| group_json(map, group, index)).collect(),
    }
}

/// One heading a queue's rows sit under: `of_node` is the headline node
/// the rows' edges point at, `None` for the orphan group - the rows
/// whose edges reach no headline.
struct RawGroup<'a> {
    of_node: Option<&'a Node>,
    rows: Vec<&'a Node>,
}

/// The first headline node an outgoing edge of `claim` reaches,
/// checking the schema's edge kinds in order - the core names no kind,
/// so this is the one rule the review draws from the schema's own
/// order rather than a settlement pair.
fn heading_of<'a>(map: &'a Map, claim: &Node) -> Option<&'a Node> {
    let headline_kinds = &map.schema().headline_kinds;
    map.schema().edge_kinds.iter().find_map(|edge_kind| {
        map.linked(claim.id, &edge_kind.name, Dir::From)
            .into_iter()
            .find(|node| headline_kinds.contains(&node.kind))
    })
}

/// The non-headline nodes with an edge into `heading` - the
/// alternatives weighed against it, whatever kind of node they are.
fn related_to<'a>(map: &'a Map, heading: &Node) -> Vec<&'a Node> {
    let headline_kinds = &map.schema().headline_kinds;
    map.schema()
        .edge_kinds
        .iter()
        .flat_map(|edge_kind| map.linked(heading.id, &edge_kind.name, Dir::To))
        .filter(|node| !headline_kinds.contains(&node.kind))
        .collect()
}

/// `claims` grouped by the first headline node each one's edges reach:
/// a claim that reaches none is its own group, unless something else
/// in the cut reaches it - then it heads that group instead. Groups
/// sort by their heading's `added_at`, so nothing here orders one edge
/// kind ahead of another the way a `reopens` chain once did.
fn grouped<'a>(map: &'a Map, claims: &[&'a Node]) -> Vec<RawGroup<'a>> {
    let mut rows_by_heading: HashMap<NodeId, Vec<&'a Node>> = HashMap::new();
    let mut unheaded: Vec<&'a Node> = Vec::new();

    for &claim in claims {
        match heading_of(map, claim) {
            Some(heading) => rows_by_heading.entry(heading.id).or_default().push(claim),
            None => unheaded.push(claim),
        }
    }

    // A heading that is itself in the cut is a row too, first in its
    // own group, so its last change and its Wrong stay reachable.
    for claim in unheaded {
        let rows = rows_by_heading.entry(claim.id).or_default();
        if !rows.iter().any(|row| row.id == claim.id) {
            rows.insert(0, claim);
        }
    }

    let mut groups: Vec<RawGroup<'a>> = rows_by_heading
        .into_iter()
        .filter_map(|(id, rows)| map.node(id).map(|heading| RawGroup { of_node: Some(heading), rows }))
        .collect();

    groups.sort_by_key(|group| group.of_node.expect("every group here has a heading").added_at);
    groups
}

/// One group as JSON: its heading, `None` for the orphan group and for
/// a group whose only row is its own heading, and its rows by
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
    let related: Vec<&Node> = group.of_node.map(|node| related_to(map, node)).unwrap_or_default();
    Group {
        heading,
        claims: rows.iter().map(|node| row_json(map, node, &related, index)).collect(),
    }
}

/// A node's short id and name.
fn node_ref(map: &Map, node: &Node) -> NodeRef {
    NodeRef {
        id: map.short_id(node.id).unwrap_or_default(),
        name: node.name.clone(),
    }
}

/// Every edge touching `node`, named by kind and direction, and the
/// node on its other end.
fn edges_of(map: &Map, node: &Node) -> Vec<EdgeRef> {
    map.edges()
        .iter()
        .filter(|edge| edge.from == node.id || edge.to == node.id)
        .filter_map(|edge| {
            if edge.from == node.id {
                map.node(edge.to).map(|other| EdgeRef {
                    kind: edge.kind.clone(),
                    dir: "from",
                    node: node_ref(map, other),
                })
            } else {
                map.node(edge.from).map(|other| EdgeRef {
                    kind: edge.kind.clone(),
                    dir: "to",
                    node: node_ref(map, other),
                })
            }
        })
        .collect()
}

/// One row: the claim itself, every edge that touches it, and the
/// alternatives related to the group's heading, computed once per group
/// and handed to every row in it.
fn row_json(map: &Map, node: &Node, related: &[&Node], index: &EventIndex) -> Row {
    Row {
        base: option_json(map, node, index),
        added_at: node.added_at.to_string(),
        edges: edges_of(map, node),
        related: related.iter().map(|node| option_json(map, node, index)).collect(),
    }
}

/// One alternative under a row's `related`, or the claim row itself.
fn option_json(map: &Map, node: &Node, index: &EventIndex) -> OptionRow {
    OptionRow {
        id: map.short_id(node.id).unwrap_or_default(),
        kind: node.kind.clone(),
        name: node.name.clone(),
        why: node.properties.get("why").cloned(),
        changed_by: node.changed_by.name(),
        changed_why: node.changed_why.clone(),
        changed_at: node.changed_at.to_string(),
        sources: index.sources_json(&node.sources),
    }
}

#[cfg(test)]
mod tests;
