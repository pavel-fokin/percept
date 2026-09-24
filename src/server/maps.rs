//! `GET /api/maps/{id}` - one project's cognitive map, whole: every
//! node and edge it folds to. `id` is the map's own `MapId`, the same
//! one `GET /api/projects` lists per map - never its schema name,
//! which repeats across projects. Unlike `server::events`, `root`
//! names the project to answer for - like `GET /api/projects` and
//! unlike `GET /api/events`, this route answers for any root the log
//! carries, not only this server's own.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::{map_id_for, EventLog, Map, Node, Schemas};
use crate::mapstore;
use crate::server::events::Error;
use crate::store;

/// `GET /api/maps/{id}`'s query string. `root` is required - a map is
/// always one project's.
#[derive(Deserialize)]
pub struct Params {
    root: Option<String>,
}

/// `GET /api/maps/{id}`'s body: the map's own name, purpose, and
/// every node kind the schema declares, and its nodes and edges named
/// by short id. `params.root`'s own schemas declare the map's name,
/// never this server's project.
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
        .iter()
        .find_map(|schema| match map_id_for(schema.name(), own.iter().copied()) {
            Ok(Some(found)) if found == id => Some(Ok(schema.name().to_string())),
            Ok(_) => None,
            Err(err) => Some(Err(err)),
        })
        .transpose()
        .map_err(|err| Error::Internal(err.to_string()))?
        .ok_or_else(|| Error::NotFound(format!("no map with id {}", id.as_uuid())))?;
    let map = mapstore::fold_map_at(&schemas, &name, &events, &root)
        .map_err(|err| Error::Internal(err.to_string()))?;

    Ok(body(&map, &root))
}

fn body(map: &Map, root: &std::path::Path) -> Value {
    let schema = map.schema();
    let kinds: Vec<Value> = schema
        .node_kinds()
        .iter()
        .map(|kind| json!({ "kind": kind.kind(), "prefix": kind.prefix() }))
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
                "properties": properties_json(map, node),
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
    let heads: Vec<Value> = mapstore::heads(map).into_iter().map(|node| json!(short_id(map, node))).collect();
    json!({
        "map": {
            "id": map.id().as_uuid().to_string(),
            "name": schema.name(),
            "purpose": schema.purpose(),
        },
        "kinds": kinds,
        "nodes": nodes,
        "edges": edges,
        "heads": heads,
        "project": root.to_string_lossy(),
    })
}

/// `node`'s properties as `Map::properties` gives them - a closed
/// list's default among them when `node` carries none of its own -
/// as a JSON object, in that same declared order.
fn properties_json(map: &Map, node: &Node) -> Value {
    let properties: serde_json::Map<String, Value> = mapstore::properties_map(map, node)
        .into_iter()
        .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
        .collect();
    Value::Object(properties)
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
