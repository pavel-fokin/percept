//! A cognitive map on the wire: folding one from the log and printing
//! it as JSONL, so `maps show` pipes into `jq` the way `events search`
//! does.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::percept::{Edge, EventKind, EventQuery, EventSearch, Map, Node, Schema};
use crate::store::event::ids;

/// Folds `schema`'s map from every map event in `search`.
pub fn fold_map(
    search: &dyn EventSearch,
    schema: &'static Schema,
) -> Result<Map, Box<dyn std::error::Error>> {
    let query = EventQuery {
        kinds: vec![
            EventKind::NodeAdded,
            EventKind::NodeRemoved,
            EventKind::EdgeAdded,
            EventKind::EdgeRemoved,
        ],
        ..Default::default()
    };
    let events = search.search(&query)?;
    Ok(Map::fold(schema, &events)?)
}

#[derive(Serialize)]
struct MapLine {
    map: &'static str,
    nodes: usize,
    edges: usize,
}

#[derive(Serialize)]
struct NodeLine<'a> {
    node: String,
    kind: &'a str,
    name: &'a str,
    properties: &'a BTreeMap<String, String>,
    sources: Vec<String>,
}

#[derive(Serialize)]
struct EdgeLine<'a> {
    edge: &'a str,
    from: String,
    to: String,
    sources: Vec<String>,
}

/// One line naming a map and its size, for `maps list`.
pub fn encode_map(map: &Map) -> String {
    serde_json::to_string(&MapLine {
        map: map.schema().name,
        nodes: map.nodes().len(),
        edges: map.edges().len(),
    })
    .expect("MapLine always serializes")
}

pub fn encode_node(node: &Node) -> String {
    serde_json::to_string(&NodeLine {
        node: node.id.as_uuid().to_string(),
        kind: &node.kind,
        name: &node.name,
        properties: &node.properties,
        sources: ids(&node.sources),
    })
    .expect("NodeLine always serializes")
}

pub fn encode_edge(edge: &Edge) -> String {
    serde_json::to_string(&EdgeLine {
        edge: &edge.kind,
        from: edge.from.as_uuid().to_string(),
        to: edge.to.as_uuid().to_string(),
        sources: ids(&edge.sources),
    })
    .expect("EdgeLine always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{EventId, NodeId};

    #[test]
    fn a_node_line_carries_its_id_and_sources_as_uuids() {
        let node = Node {
            id: NodeId::new(),
            kind: "evidence".to_string(),
            name: "Built both".to_string(),
            properties: BTreeMap::from([("summary".to_string(), "side by side".to_string())]),
            sources: vec![EventId::new()],
        };

        let line: serde_json::Value = serde_json::from_str(&encode_node(&node)).unwrap();

        assert_eq!(line["node"], node.id.as_uuid().to_string());
        assert_eq!(line["kind"], "evidence");
        assert_eq!(line["name"], "Built both");
        assert_eq!(line["properties"]["summary"], "side by side");
        assert_eq!(line["sources"][0], node.sources[0].as_uuid().to_string());
    }

    #[test]
    fn an_edge_line_names_its_kind_under_edge() {
        let edge = Edge {
            kind: "supports".to_string(),
            from: NodeId::new(),
            to: NodeId::new(),
            sources: Vec::new(),
        };

        let line: serde_json::Value = serde_json::from_str(&encode_edge(&edge)).unwrap();

        assert_eq!(line["edge"], "supports");
        assert_eq!(line["from"], edge.from.as_uuid().to_string());
        assert_eq!(line["sources"], serde_json::json!([]));
    }
}
