//! `GET /api/maps/{id}` - one project's cognitive map, cut to what the
//! reader asked for: the overview (the nodes that head it, in map order)
//! with no `around`, or the cut `Map::around` gives around one node.
//! `id` is the map's own `MapId`, the same one `GET /api/projects`
//! lists per map - never its schema name, which repeats across
//! projects. Unlike `server::events`, `root` names the project to
//! answer for - like `GET /api/projects` and unlike `GET /api/events`,
//! this route answers for any root the log carries, not only this
//! server's own.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::{map_id_for, EventLog, Map, Node, NodeRef, Selection};
use crate::mapstore::{self, outline};
use crate::server::events::Error;
use crate::store;

/// How many edges out from `around` the cut reaches when the query
/// carries no `depth` of its own.
const DEFAULT_DEPTH: usize = 1;

/// `GET /api/maps/{id}`'s query string. `root` is required - a map is
/// always one project's - `around` and `depth` are not, and `depth`
/// only means anything alongside `around`.
#[derive(Deserialize)]
pub struct Params {
    root: Option<String>,
    around: Option<String>,
    depth: Option<usize>,
}

/// `GET /api/maps/{id}`'s body: the map's own name, purpose, and
/// every node kind the schema declares, the cut's nodes and edges
/// named by short id, and the four counts `Fragment` reports.
/// `params.root`'s own schemas declare the map's name, never this
/// server's project.
pub fn get(log: &dyn EventLog, id: &str, params: Params) -> Result<Value, Error> {
    let id = store::parse_map_id(id).map_err(|err| Error::Bad(err.to_string()))?;
    let root = params.root.ok_or_else(|| Error::Bad("root is required".to_string()))?;
    let root = PathBuf::from(root);

    let events = log.load().map_err(|err| Error::Internal(err.to_string()))?;
    let own: Vec<_> = mapstore::of_path(&events, &root).collect();
    if own.is_empty() {
        return Err(Error::NotFound(format!("no events for root {}", root.display())));
    }

    let schemas = mapstore::load_schemas(&root).map_err(|err| Error::Internal(err.to_string()))?;
    // `map_id_for` only scans `own` for the schema's `map.created` event,
    // far cheaper than folding a schema's whole map - so the schema `id`
    // names is found before anything is folded, and only that one map
    // pays the fold.
    let name = schemas
        .folded()
        .find_map(|schema| match map_id_for(&schema.name, own.iter().copied()) {
            Ok(Some(found)) if found == id => Some(Ok(schema.name.clone())),
            Ok(_) => None,
            Err(err) => Some(Err(err)),
        })
        .transpose()
        .map_err(|err| Error::Internal(err.to_string()))?
        .ok_or_else(|| Error::NotFound(format!("no map with id {}", id.as_uuid())))?;
    let map = mapstore::fold_map_at(&schemas, &name, &events, &root)
        .map_err(|err| Error::Internal(err.to_string()))?;
    let node_ref;
    let heads: Vec<crate::core::NodeId> = match params.around {
        Some(_) => Vec::new(),
        None => outline::roots(&map).iter().map(|node| node.id).collect(),
    };
    let selection = match params.around.as_deref() {
        Some(around) => {
            let id = map.resolve_str(around).map_err(|err| Error::NotFound(err.to_string()))?;
            let node = map.node(id).expect("resolve_str only ever names a node the map holds");
            node_ref = NodeRef::from(node);
            Selection {
                around: Some((&node_ref, params.depth.unwrap_or(DEFAULT_DEPTH))),
                ..Selection::default()
            }
        }
        // No `around`: the overview is what heads the map - the nodes
        // nobody claims. Which those are is the outline's rule, not a
        // cut any kind describes, so the server names them one by one
        // and `Fragment`'s counts report the whole map behind them.
        None => Selection {
            nodes: &heads,
            ..Selection::default()
        },
    };
    let fragment = map.select(&selection).map_err(|err| Error::Internal(err.to_string()))?;

    Ok(body(fragment.map(), &fragment, &root))
}

fn body(map: &Map, fragment: &crate::core::Fragment, root: &std::path::Path) -> Value {
    let schema = map.schema();
    let kinds: Vec<Value> = schema
        .node_kinds
        .iter()
        .map(|kind| json!({ "kind": kind.kind, "prefix": kind.prefix }))
        .collect();
    let nodes: Vec<Value> = map
        .nodes()
        .iter()
        .map(|node| {
            json!({
                "id": short_id(map, node),
                "node": node.id.as_uuid().to_string(),
                "kind": node.kind,
                "name": node.name,
                "properties": node.properties,
            })
        })
        .collect();
    let edges: Vec<Value> = map
        .edges()
        .iter()
        .map(|edge| {
            let from = map.node(edge.from).expect("an edge's from end is a node of its map");
            let to = map.node(edge.to).expect("an edge's to end is a node of its map");
            json!({ "kind": edge.kind, "from": short_id(map, from), "to": short_id(map, to) })
        })
        .collect();
    json!({
        "map": {
            "id": map.id().as_uuid().to_string(),
            "name": schema.name,
            "purpose": schema.purpose,
        },
        "kinds": kinds,
        "nodes": nodes,
        "edges": edges,
        "shown_nodes": map.nodes().len(),
        "total_nodes": fragment.total_nodes(),
        "total_edges": fragment.total_edges(),
        "boundary_edges": fragment.boundary_edges(),
        "project": root.to_string_lossy(),
    })
}

/// `node`'s short id, or the `kind:name` form when the map minted none -
/// the same fallback `mapstore::map::node_ref` gives an edge's ends,
/// duplicated here since that helper is private to its module and this
/// route's edge shape - naming short ids on both ends, no `stamp` -
/// differs deliberately from `mapstore`'s own.
fn short_id(map: &Map, node: &Node) -> String {
    map.short_id(node.id).unwrap_or_else(|| format!("{}:{}", node.kind, node.name))
}

#[cfg(test)]
mod tests;
