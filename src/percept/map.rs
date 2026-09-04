//! A cognitive map: a graph the model builds from the log and reasons
//! over. A `Schema` says which node and edge kinds a map allows; a
//! `Map` is folded from the map events in the log, so the log stays
//! the one source of truth and a map is a view a reader rebuilds.
//! `Map::apply` turns a `Mutation` into the `Payload` that records it.
//! Every writer - the CLI, the model's tool - goes through `apply`, so
//! one place holds the rules.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use super::{Event, EventId, Payload};
use crate::shared::Id;

/// Which node and edge kinds a map allows. Data, not an enum: adding a
/// map is adding a value.
#[derive(Debug, PartialEq, Eq)]
pub struct Schema {
    pub name: &'static str,
    pub node_kinds: &'static [&'static str],
    pub edge_kinds: &'static [&'static str],
    /// The node kinds worth a reader's attention without opening the
    /// whole map - what `MapShape::Headlines` sends.
    pub headline_kinds: &'static [&'static str],
}

/// The decision map: what was asked, what was weighed, what was chosen
/// and on what grounds.
pub const DECISIONS: Schema = Schema {
    name: "decisions",
    node_kinds: &["question", "option", "evidence", "decision"],
    edge_kinds: &["supports", "contradicts", "resolves"],
    headline_kinds: &["question", "decision"],
};

/// Every map percept knows. One map per schema, named after it.
pub const SCHEMAS: &[&Schema] = &[&DECISIONS];

impl Schema {
    /// The schema `name` names, or the error every boundary that takes
    /// a map name reports.
    pub fn find(name: &str) -> Result<&'static Schema, MapError> {
        SCHEMAS
            .iter()
            .copied()
            .find(|schema| schema.name == name)
            .ok_or_else(|| MapError::UnknownMap(name.to_string()))
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

/// A node as a writer names it: kind and quoted name, never the id.
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}", self.kind, self.name)
    }
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

/// Points at a node the way a writer knows it - by kind and name -
/// rather than by id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRef {
    pub kind: String,
    pub name: String,
}

impl fmt::Display for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}", self.kind, self.name)
    }
}

/// One change a writer asks for. Names nodes by `NodeRef`; the
/// `Payload` that `apply` returns carries ids.
pub enum Mutation {
    AddNode {
        kind: String,
        name: String,
        properties: BTreeMap<String, String>,
        sources: Vec<EventId>,
    },
    RemoveNode {
        node: NodeRef,
        reason: String,
        sources: Vec<EventId>,
    },
    AddEdge {
        kind: String,
        from: NodeRef,
        to: NodeRef,
        sources: Vec<EventId>,
    },
    RemoveEdge {
        kind: String,
        from: NodeRef,
        to: NodeRef,
        sources: Vec<EventId>,
    },
}

/// Why a mutation, or a stored event, doesn't fit its map. Each names
/// the rule and the value that broke it. `apply` checks a mutation
/// before it becomes an event, so a stored event that breaks a rule
/// means a race between writers or a hand-edited log. Edge ends are
/// carried as labels - kind and name - the way a reader knows them.
#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    UnknownMap(String),
    UnknownNodeKind {
        map: &'static Schema,
        kind: String,
    },
    UnknownEdgeKind {
        map: &'static Schema,
        kind: String,
    },
    /// A name that is blank would be a node nobody can point at.
    BlankName,
    DuplicateNode {
        kind: String,
        name: String,
    },
    NoSuchNode(NodeRef),
    /// A stored event names a node id the fold never saw.
    NoSuchNodeId(NodeId),
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
            Self::UnknownMap(name) => write!(
                f,
                "no map named {name:?}; maps are {}",
                SCHEMAS
                    .iter()
                    .map(|schema| schema.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::UnknownNodeKind { map, kind } => write!(
                f,
                "no node kind {kind:?} in map {:?}; kinds are {}",
                map.name,
                map.node_kinds.join(", ")
            ),
            Self::UnknownEdgeKind { map, kind } => write!(
                f,
                "no edge kind {kind:?} in map {:?}; kinds are {}",
                map.name,
                map.edge_kinds.join(", ")
            ),
            Self::BlankName => write!(f, "a node's name must not be blank"),
            Self::DuplicateNode { kind, name } => {
                write!(f, "{kind} {name:?} is already in the map")
            }
            Self::NoSuchNode(node) => write!(f, "no {node} in the map"),
            Self::NoSuchNodeId(id) => write!(f, "no node with id {}", id.as_uuid()),
            Self::DuplicateEdge { kind, from, to } => {
                write!(f, "{from} {kind} {to} is already in the map")
            }
            Self::NoSuchEdge { kind, from, to } => write!(f, "no edge {from} {kind} {to}"),
            Self::Rejected { event, error } => {
                write!(f, "event {} does not fit its map: {error}", event.as_uuid())
            }
        }
    }
}

impl std::error::Error for MapError {}

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
    pub fn fold<'a>(
        schema: &'static Schema,
        events: impl IntoIterator<Item = &'a Event>,
    ) -> Result<Self, MapError> {
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

    /// Every map percept knows, folded from `events`.
    pub fn fold_all<'a>(
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Self>, MapError> {
        SCHEMAS
            .iter()
            .map(|schema| Self::fold(schema, events.clone()))
            .collect()
    }

    pub fn schema(&self) -> &'static Schema {
        self.schema
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The nodes of the schema's headline kinds, in map order - what a
    /// reader sees of the map before opening it.
    pub fn headlines(&self) -> impl Iterator<Item = &Node> {
        let kinds = self.schema.headline_kinds;
        self.nodes
            .iter()
            .filter(move |node| kinds.contains(&node.kind.as_str()))
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

    /// The map cut to nodes of `kinds`, keeping only the edges whose
    /// both ends survived. A kind the schema lacks is an error, so an
    /// empty result means the map holds none of that kind.
    pub fn keep_kinds(&self, kinds: &[String]) -> Result<Self, MapError> {
        for kind in kinds {
            self.check_node_kind(kind)?;
        }
        let nodes: Vec<Node> = self
            .nodes
            .iter()
            .filter(|node| kinds.contains(&node.kind))
            .cloned()
            .collect();
        Ok(self.cut_to(nodes))
    }

    /// A copy holding `nodes` and only the edges that join two of them.
    /// An edge to a node outside the cut is not a fact of the cut.
    fn cut_to(&self, nodes: Vec<Node>) -> Self {
        let kept: HashSet<NodeId> = nodes.iter().map(|node| node.id).collect();
        let edges = self
            .edges
            .iter()
            .filter(|edge| kept.contains(&edge.from) && kept.contains(&edge.to))
            .cloned()
            .collect();
        Self {
            schema: self.schema,
            nodes,
            edges,
        }
    }

    /// Checks `mutation` against the schema and the map's current
    /// state, applies it, and returns the `Payload` that records it.
    /// The caller commits that payload; the map is already updated, so
    /// a batch can check each step against the ones before it.
    pub fn apply(&mut self, mutation: Mutation) -> Result<Payload, MapError> {
        let map = self.schema.name.to_string();
        let payload = match mutation {
            Mutation::AddNode {
                kind,
                name,
                properties,
                sources,
            } => Payload::NodeAdded {
                map,
                node: NodeId::new(),
                kind,
                name,
                properties,
                sources,
            },
            Mutation::RemoveNode {
                node,
                reason,
                sources,
            } => Payload::NodeRemoved {
                map,
                node: self.resolve(node)?,
                reason,
                sources,
            },
            Mutation::AddEdge {
                kind,
                from,
                to,
                sources,
            } => Payload::EdgeAdded {
                map,
                kind,
                from: self.resolve(from)?,
                to: self.resolve(to)?,
                sources,
            },
            Mutation::RemoveEdge {
                kind,
                from,
                to,
                sources,
            } => Payload::EdgeRemoved {
                map,
                kind,
                from: self.resolve(from)?,
                to: self.resolve(to)?,
                sources,
            },
        };
        self.replay(&payload)?;
        Ok(payload)
    }

    /// Applies one recorded change - every rule a map enforces lives
    /// here, so a fold and `apply` agree. Removing a node drops the
    /// edges that touch it: an edge to nothing is not a fact.
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
                if name.trim().is_empty() {
                    return Err(MapError::BlankName);
                }
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
                        from: self.label(*from),
                        to: self.label(*to),
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
                        from: self.label(*from),
                        to: self.label(*to),
                    });
                }
                self.edges
                    .retain(|e| !(e.kind == *kind && e.from == *from && e.to == *to));
            }
            _ => {}
        }
        Ok(())
    }

    fn resolve(&self, node: NodeRef) -> Result<NodeId, MapError> {
        self.find(&node.kind, &node.name)
            .map(|n| n.id)
            .ok_or(MapError::NoSuchNode(node))
    }

    fn edge(&self, kind: &str, from: NodeId, to: NodeId) -> Option<&Edge> {
        self.edges
            .iter()
            .find(|e| e.kind == kind && e.from == from && e.to == to)
    }

    /// A node as `Display` names it; the id when the map has no such
    /// node.
    fn label(&self, id: NodeId) -> String {
        self.node(id)
            .map_or_else(|| id.as_uuid().to_string(), Node::to_string)
    }

    fn check_node_kind(&self, kind: &str) -> Result<(), MapError> {
        if self.schema.node_kinds.contains(&kind) {
            Ok(())
        } else {
            Err(MapError::UnknownNodeKind {
                map: self.schema,
                kind: kind.to_string(),
            })
        }
    }

    fn check_edge_kind(&self, kind: &str) -> Result<(), MapError> {
        if self.schema.edge_kinds.contains(&kind) {
            Ok(())
        } else {
            Err(MapError::UnknownEdgeKind {
                map: self.schema,
                kind: kind.to_string(),
            })
        }
    }

    fn check_node_id(&self, id: NodeId) -> Result<(), MapError> {
        if self.node(id).is_some() {
            Ok(())
        } else {
            Err(MapError::NoSuchNodeId(id))
        }
    }
}

/// The map as text for a model to read: one line per node, then one
/// per edge, nodes named by kind and name - the way a writer refers to
/// them - never by id. Names and property values are quoted, so a
/// newline inside one stays inside its line. Empty for an empty map.
impl fmt::Display for Map {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for node in &self.nodes {
            write!(f, "- {node}")?;
            let mut sep = ": ";
            for (key, value) in &node.properties {
                write!(f, "{sep}{key}: {value:?}")?;
                sep = "; ";
            }
            writeln!(f)?;
        }
        for edge in &self.edges {
            writeln!(
                f,
                "- {} {} {}",
                self.label(edge.from),
                edge.kind,
                self.label(edge.to)
            )?;
        }
        Ok(())
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
mod tests;
