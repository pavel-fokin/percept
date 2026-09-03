//! A cognitive map: a graph the model builds from the log and reasons
//! over. A `Schema` says which node and edge kinds a map allows; a
//! `Map` is folded from the map events in the log, so the log stays
//! the one source of truth and a map is a view a reader rebuilds.

use std::collections::BTreeMap;
use std::fmt;

use super::{Event, EventId, Payload};
use crate::shared::Id;

/// Which node and edge kinds a map allows. Data, not an enum: adding a
/// map is adding a value.
pub struct Schema {
    pub name: &'static str,
    pub node_kinds: &'static [&'static str],
    pub edge_kinds: &'static [&'static str],
}

/// The decision map: what was asked, what was weighed, what was chosen
/// and on what grounds.
pub const DECISIONS: Schema = Schema {
    name: "decisions",
    node_kinds: &["question", "option", "evidence", "decision"],
    edge_kinds: &["supports", "contradicts", "resolves"],
};

/// Every map percept knows. One map per schema, named after it.
pub const SCHEMAS: &[&Schema] = &[&DECISIONS];

impl Schema {
    pub fn named(name: &str) -> Option<&'static Schema> {
        SCHEMAS.iter().copied().find(|schema| schema.name == name)
    }
}

/// Identifies a node in a cognitive map.
pub type NodeId = Id<Node>;

/// One node of a map. `name` is unique within its map and kind, so a
/// writer can point at a node by what it is called; `id` is what
/// history keeps.
#[derive(Clone)]
pub struct Node {
    pub id: NodeId,
    pub kind: String,
    pub name: String,
    pub properties: BTreeMap<String, String>,
    pub sources: Vec<EventId>,
}

/// One edge of a map. Carries no id: `kind`, `from`, and `to` identify
/// it, and two edges alike would be one fact stated twice.
#[derive(Clone)]
pub struct Edge {
    pub kind: String,
    pub from: NodeId,
    pub to: NodeId,
    pub sources: Vec<EventId>,
}

/// Why a stored event doesn't fit its map. Each names the rule and the
/// value that broke it. Writers check these rules before they append,
/// so a stored event that breaks one means a race between writers or
/// a hand-edited log.
#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    UnknownNodeKind {
        map: &'static str,
        kind: String,
    },
    UnknownEdgeKind {
        map: &'static str,
        kind: String,
    },
    DuplicateNode {
        kind: String,
        name: String,
    },
    NoSuchNode(NodeId),
    DuplicateEdge {
        kind: String,
        from: String,
        to: String,
    },
    NoSuchEdge {
        kind: String,
        from: String,
        to: String,
    },
    /// Wraps any of the above with the event that broke the rule.
    Rejected {
        event: EventId,
        error: Box<MapError>,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNodeKind { map, kind } => write!(
                f,
                "no node kind {kind:?} in map {map:?}; kinds are {}",
                allowed(map, |schema| schema.node_kinds)
            ),
            Self::UnknownEdgeKind { map, kind } => write!(
                f,
                "no edge kind {kind:?} in map {map:?}; kinds are {}",
                allowed(map, |schema| schema.edge_kinds)
            ),
            Self::DuplicateNode { kind, name } => {
                write!(f, "{kind} {name:?} is already in the map")
            }
            Self::NoSuchNode(id) => write!(f, "no node with id {}", id.as_uuid()),
            Self::DuplicateEdge { kind, from, to } => {
                write!(f, "{from:?} {kind} {to:?} is already in the map")
            }
            Self::NoSuchEdge { kind, from, to } => write!(f, "no edge {from:?} {kind} {to:?}"),
            Self::Rejected { event, error } => {
                write!(f, "event {} does not fit its map: {error}", event.as_uuid())
            }
        }
    }
}

impl std::error::Error for MapError {}

/// The kinds a map allows, for an error naming one it doesn't.
fn allowed(map: &str, kinds: fn(&Schema) -> &'static [&'static str]) -> String {
    Schema::named(map).map_or_else(String::new, |schema| kinds(schema).join(", "))
}

/// A map folded from the log. Holds every node and edge still present;
/// what was removed lives only in the events.
pub struct Map {
    schema: &'static Schema,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl Map {
    pub fn empty(schema: &'static Schema) -> Self {
        Self {
            schema,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Folds the events that belong to `schema`'s map, in the order
    /// given, which must be log order. Events for other maps and of
    /// other kinds are skipped. An event that breaks a rule is an
    /// error naming it, not skipped: silently dropping it would hide
    /// that something went wrong at write time.
    pub fn fold(schema: &'static Schema, events: &[Event]) -> Result<Self, MapError> {
        let mut map = Self::empty(schema);
        for event in events {
            if map_of(event.payload()) != Some(schema.name) {
                continue;
            }
            map.replay(event.payload())
                .map_err(|error| MapError::Rejected {
                    event: event.id(),
                    error: Box::new(error),
                })?;
        }
        Ok(map)
    }

    pub fn schema(&self) -> &'static Schema {
        self.schema
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn find(&self, kind: &str, name: &str) -> Option<&Node> {
        self.nodes
            .iter()
            .find(|node| node.kind == kind && node.name == name)
    }

    /// Applies one recorded change. Removing a node drops the edges
    /// that touch it: an edge to nothing is not a fact.
    fn replay(&mut self, payload: &Payload) -> Result<(), MapError> {
        match payload {
            Payload::NodeAdded {
                node,
                kind,
                name,
                properties,
                sources,
                ..
            } => {
                self.check_node_kind(kind)?;
                if self.find(kind, name).is_some() {
                    return Err(MapError::DuplicateNode {
                        kind: kind.clone(),
                        name: name.clone(),
                    });
                }
                self.nodes.push(Node {
                    id: *node,
                    kind: kind.clone(),
                    name: name.clone(),
                    properties: properties.clone(),
                    sources: sources.clone(),
                });
            }
            Payload::NodeRemoved { node, .. } => {
                self.check_node_id(*node)?;
                self.nodes.retain(|n| n.id != *node);
                self.edges.retain(|e| e.from != *node && e.to != *node);
            }
            Payload::EdgeAdded {
                kind,
                from,
                to,
                sources,
                ..
            } => {
                self.check_edge_kind(kind)?;
                self.check_node_id(*from)?;
                self.check_node_id(*to)?;
                if self.edge(kind, *from, *to).is_some() {
                    return Err(MapError::DuplicateEdge {
                        kind: kind.clone(),
                        from: self.name_of(*from),
                        to: self.name_of(*to),
                    });
                }
                self.edges.push(Edge {
                    kind: kind.clone(),
                    from: *from,
                    to: *to,
                    sources: sources.clone(),
                });
            }
            Payload::EdgeRemoved { kind, from, to, .. } => {
                if self.edge(kind, *from, *to).is_none() {
                    return Err(MapError::NoSuchEdge {
                        kind: kind.clone(),
                        from: self.name_of(*from),
                        to: self.name_of(*to),
                    });
                }
                self.edges
                    .retain(|e| !(e.kind == *kind && e.from == *from && e.to == *to));
            }
            _ => {}
        }
        Ok(())
    }

    fn edge(&self, kind: &str, from: NodeId, to: NodeId) -> Option<&Edge> {
        self.edges
            .iter()
            .find(|e| e.kind == kind && e.from == from && e.to == to)
    }

    /// A node's name for an error message; the id when the map has no
    /// such node.
    fn name_of(&self, id: NodeId) -> String {
        self.node(id)
            .map_or_else(|| id.as_uuid().to_string(), |n| n.name.clone())
    }

    fn check_node_kind(&self, kind: &str) -> Result<(), MapError> {
        if self.schema.node_kinds.contains(&kind) {
            Ok(())
        } else {
            Err(MapError::UnknownNodeKind {
                map: self.schema.name,
                kind: kind.to_string(),
            })
        }
    }

    fn check_edge_kind(&self, kind: &str) -> Result<(), MapError> {
        if self.schema.edge_kinds.contains(&kind) {
            Ok(())
        } else {
            Err(MapError::UnknownEdgeKind {
                map: self.schema.name,
                kind: kind.to_string(),
            })
        }
    }

    fn check_node_id(&self, id: NodeId) -> Result<(), MapError> {
        if self.node(id).is_some() {
            Ok(())
        } else {
            Err(MapError::NoSuchNode(id))
        }
    }
}

/// Which map a payload changes, if it changes one.
pub fn map_of(payload: &Payload) -> Option<&str> {
    match payload {
        Payload::NodeAdded { map, .. }
        | Payload::NodeRemoved { map, .. }
        | Payload::EdgeAdded { map, .. }
        | Payload::EdgeRemoved { map, .. } => Some(map),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::Actor;

    fn committed(payload: Payload) -> Event {
        Event::new(Actor::User, "test".to_string(), None, payload)
    }

    fn node_added(map: &str, node: NodeId, kind: &str, name: &str) -> Event {
        committed(Payload::NodeAdded {
            map: map.to_string(),
            node,
            kind: kind.to_string(),
            name: name.to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        })
    }

    fn edge_added(map: &str, kind: &str, from: NodeId, to: NodeId) -> Event {
        committed(Payload::EdgeAdded {
            map: map.to_string(),
            kind: kind.to_string(),
            from,
            to,
            sources: Vec::new(),
        })
    }

    fn node_removed(map: &str, node: NodeId) -> Event {
        committed(Payload::NodeRemoved {
            map: map.to_string(),
            node,
            reason: "gone".to_string(),
            sources: Vec::new(),
        })
    }

    /// The decision from the ADR: a question, an option, a decision,
    /// and the edge that resolves the question.
    fn rust_over_go() -> ([NodeId; 3], Vec<Event>) {
        let ids = [NodeId::new(), NodeId::new(), NodeId::new()];
        let events = vec![
            node_added("decisions", ids[0], "question", "Which language?"),
            node_added("decisions", ids[1], "option", "Rust"),
            node_added("decisions", ids[2], "decision", "Rust over Go"),
            edge_added("decisions", "resolves", ids[2], ids[0]),
        ];
        (ids, events)
    }

    fn rejected_with(err: MapError, expected: EventId) -> MapError {
        match err {
            MapError::Rejected { event, error } => {
                assert!(event == expected);
                *error
            }
            other => panic!("expected Rejected, got {other}"),
        }
    }

    #[test]
    fn a_fold_holds_every_node_and_edge_still_present() {
        let (ids, events) = rust_over_go();

        let map = Map::fold(&DECISIONS, &events).unwrap();

        assert_eq!(map.nodes().len(), 3);
        assert_eq!(map.edges().len(), 1);
        assert!(map.find("decision", "Rust over Go").unwrap().id == ids[2]);
        assert!(map.edges()[0].from == ids[2]);
        assert!(map.edges()[0].to == ids[0]);
    }

    #[test]
    fn a_fold_skips_other_maps_and_other_kinds() {
        let (_, mut events) = rust_over_go();
        events.push(committed(Payload::MessageReceived {
            content: "hi".to_string(),
        }));
        events.push(node_added("tasks", NodeId::new(), "goal", "Ship"));

        let map = Map::fold(&DECISIONS, &events).unwrap();

        assert_eq!(map.nodes().len(), 3);
    }

    #[test]
    fn removing_a_node_drops_its_edges() {
        let (ids, mut events) = rust_over_go();
        events.push(node_removed("decisions", ids[0]));

        let map = Map::fold(&DECISIONS, &events).unwrap();

        assert_eq!(map.nodes().len(), 2);
        assert!(map.edges().is_empty());
    }

    #[test]
    fn removing_an_edge_leaves_its_nodes() {
        let (ids, mut events) = rust_over_go();
        events.push(committed(Payload::EdgeRemoved {
            map: "decisions".to_string(),
            kind: "resolves".to_string(),
            from: ids[2],
            to: ids[0],
            sources: Vec::new(),
        }));

        let map = Map::fold(&DECISIONS, &events).unwrap();

        assert_eq!(map.nodes().len(), 3);
        assert!(map.edges().is_empty());
    }

    #[test]
    fn an_unknown_kind_fails_the_fold() {
        let stray = node_added("decisions", NodeId::new(), "goal", "Ship");
        let stray_id = stray.id();

        let err = Map::fold(&DECISIONS, &[stray]).err().unwrap();

        assert_eq!(
            rejected_with(err, stray_id),
            MapError::UnknownNodeKind {
                map: "decisions",
                kind: "goal".to_string()
            }
        );
        assert_eq!(
            MapError::UnknownNodeKind {
                map: "decisions",
                kind: "goal".to_string()
            }
            .to_string(),
            "no node kind \"goal\" in map \"decisions\"; kinds are question, option, evidence, decision"
        );
    }

    #[test]
    fn a_name_is_unique_within_its_kind_only() {
        let events = vec![
            node_added("decisions", NodeId::new(), "option", "Rust"),
            node_added("decisions", NodeId::new(), "decision", "Rust"),
        ];
        assert_eq!(Map::fold(&DECISIONS, &events).unwrap().nodes().len(), 2);

        let twice = node_added("decisions", NodeId::new(), "option", "Rust");
        let twice_id = twice.id();
        let mut events = events;
        events.push(twice);

        let err = Map::fold(&DECISIONS, &events).err().unwrap();

        assert_eq!(
            rejected_with(err, twice_id),
            MapError::DuplicateNode {
                kind: "option".to_string(),
                name: "Rust".to_string()
            }
        );
    }

    #[test]
    fn an_edge_needs_both_ends_and_is_stated_once() {
        let (ids, mut events) = rust_over_go();
        let dangling = edge_added("decisions", "supports", NodeId::new(), ids[1]);
        let dangling_id = dangling.id();
        let mut with_dangling = events.clone();
        with_dangling.push(dangling);

        let err = Map::fold(&DECISIONS, &with_dangling).err().unwrap();
        assert!(matches!(
            rejected_with(err, dangling_id),
            MapError::NoSuchNode(_)
        ));

        let twice = edge_added("decisions", "resolves", ids[2], ids[0]);
        let twice_id = twice.id();
        events.push(twice);

        let err = Map::fold(&DECISIONS, &events).err().unwrap();
        assert_eq!(
            rejected_with(err, twice_id),
            MapError::DuplicateEdge {
                kind: "resolves".to_string(),
                from: "Rust over Go".to_string(),
                to: "Which language?".to_string()
            }
        );
    }

    #[test]
    fn removing_an_edge_that_is_not_there_fails_the_fold() {
        let (ids, mut events) = rust_over_go();
        let stray = committed(Payload::EdgeRemoved {
            map: "decisions".to_string(),
            kind: "supports".to_string(),
            from: ids[1],
            to: ids[0],
            sources: Vec::new(),
        });
        let stray_id = stray.id();
        events.push(stray);

        let err = Map::fold(&DECISIONS, &events).err().unwrap();

        assert!(matches!(
            rejected_with(err, stray_id),
            MapError::NoSuchEdge { .. }
        ));
    }

    #[test]
    fn a_schema_is_found_by_name() {
        assert_eq!(
            Schema::named("decisions").map(|s| s.name),
            Some("decisions")
        );
        assert!(Schema::named("tasks").is_none());
    }
}
