//! A cognitive map: a graph the model builds from the log and reasons
//! over. A `Schema` says which node and edge kinds a map allows; a
//! `Map` is folded from the map events in the log, so the log stays
//! the one source of truth and a map is a view a reader rebuilds.
//! This file holds the map itself - the struct with its indexes, the
//! fold, the accessors, and the cuts a reader takes. The write path is
//! `apply`, the vocabulary a map is declared in is `schema`, and every
//! way a write can fail is `error`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::sync::Arc;

use super::{Actor, Event, EventId, Payload};
use crate::shared::{Id, Timestamp};

mod apply;
mod error;
mod schema;

use apply::highest;

pub use error::MapError;
pub use schema::{default_prefix, EdgeKind, NodeKind, Schema, Schemas};

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
    /// Every write that reached this node, in log order, the addition
    /// first. Never empty.
    pub history: Vec<Change>,
    /// This node's number within its kind, minted once when it was
    /// added - `d41` is its kind's prefix plus this. See
    /// `Payload::NodeAdded`.
    pub seq: u32,
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
    /// One entry today - the addition.
    pub history: Vec<Change>,
}

/// One write that reached a node or an edge: who, when, and on what
/// grounds. `why` is `None` on an addition - a node's own why is a
/// property - and on a plain edit that gave none.
#[derive(Clone, Debug)]
pub struct Change {
    pub actor: Actor,
    pub at: Timestamp,
    pub why: Option<String>,
}

/// A node or an edge that keeps every write that reached it. Default
/// methods read who added it, who last changed it, and who ranks
/// highest among everyone who touched it, so that logic lives once for
/// both.
pub trait Written {
    fn history(&self) -> &[Change];

    /// The addition - the first entry in `history`.
    fn added(&self) -> &Change {
        self.history().first().expect("history is never empty")
    }

    /// The last write - the last entry in `history`.
    fn changed(&self) -> &Change {
        self.history().last().expect("history is never empty")
    }

    /// The highest-ranked actor in the history.
    fn touched_by(&self) -> Actor {
        highest(self.history().iter().map(|change| change.actor)).expect("history is never empty")
    }
}

impl Written for Node {
    fn history(&self) -> &[Change] {
        &self.history
    }
}

impl Written for Edge {
    fn history(&self) -> &[Change] {
        &self.history
    }
}

/// Points at a node the way a writer knows it - by kind and name -
/// rather than by id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRef {
    pub kind: String,
    pub name: String,
}

impl From<&Node> for NodeRef {
    fn from(node: &Node) -> Self {
        Self {
            kind: node.kind.clone(),
            name: node.name.clone(),
        }
    }
}

impl fmt::Display for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}", self.kind, self.name)
    }
}

/// How much of a map a reader asked for. `around` cuts first, then
/// `since`, then `kinds`, so the three together read as "what changed
/// near this node, of these kinds". All absent is the whole map.
#[derive(Default)]
pub struct Selection<'a> {
    pub around: Option<(&'a NodeRef, usize)>,
    pub since: Option<Timestamp>,
    pub kinds: &'a [String],
}

impl Selection<'_> {
    pub fn is_whole(&self) -> bool {
        self.around.is_none() && self.since.is_none() && self.kinds.is_empty()
    }
}

/// A map cut to a `Selection`, with what the cut left out counted: the
/// whole map's size, and the edges with one end inside the cut and one
/// outside - where a reader who needs more widens from.
pub struct Fragment {
    map: Map,
    total_nodes: usize,
    total_edges: usize,
    boundary_edges: usize,
}

impl Fragment {
    pub fn map(&self) -> &Map {
        &self.map
    }

    pub fn total_nodes(&self) -> usize {
        self.total_nodes
    }

    pub fn total_edges(&self) -> usize {
        self.total_edges
    }

    pub fn boundary_edges(&self) -> usize {
        self.boundary_edges
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
    ChangeNode {
        node: NodeRef,
        name: Option<String>,
        properties: BTreeMap<String, String>,
        sources: Vec<EventId>,
        why: Option<String>,
    },
    RemoveNode {
        node: NodeRef,
        why: String,
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
        why: String,
    },
}

/// One end of an edge: the end `Map::linked` reads across, or the end
/// that broke a `WrongEdgeEnd` rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeEnd {
    From,
    To,
}

impl fmt::Display for EdgeEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::From => "from",
            Self::To => "to",
        })
    }
}

/// A map folded from the log. Holds every node and edge still present;
/// what was removed lives only in the events.
pub struct Map {
    schema: Arc<Schema>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    // Indexes over `nodes` and `edges`, kept in step by `replay`, so a
    // lookup by id, by name, or by edge is a hash rather than a scan:
    // every `apply` looks up its ends, and a code map applies tens of
    // thousands of them.
    by_id: HashMap<NodeId, usize>,
    by_name: HashMap<(String, String), NodeId>,
    // A node's short id, by kind and its minted number - what
    // `resolve_str` looks a short id up in, kept in step wherever
    // `by_name` is.
    by_seq: HashMap<(String, u32), NodeId>,
    // The next short id number `next_seq` mints per kind. Tracked apart
    // from `nodes`, and never rolled back on a `NodeRemoved`: a short
    // id is cited in chat, PR comments, and committed text, so a
    // removal freeing its number for reuse would make an old citation
    // point at the wrong node.
    next_seq_by_kind: HashMap<String, u32>,
    edge_keys: HashSet<(String, NodeId, NodeId)>,
}

impl Map {
    pub fn empty(schema: impl Into<Arc<Schema>>) -> Self {
        Self::from_parts(schema.into(), Vec::new(), Vec::new())
    }

    fn from_parts(schema: Arc<Schema>, nodes: Vec<Node>, edges: Vec<Edge>) -> Self {
        let by_id = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let by_name = nodes
            .iter()
            .map(|n| ((n.kind.clone(), n.name.clone()), n.id))
            .collect();
        let by_seq = nodes
            .iter()
            .map(|n| ((n.kind.clone(), n.seq), n.id))
            .collect();
        let mut next_seq_by_kind: HashMap<String, u32> = HashMap::new();
        for node in &nodes {
            let next = next_seq_by_kind.entry(node.kind.clone()).or_insert(1);
            *next = (*next).max(node.seq + 1);
        }
        let edge_keys = edges
            .iter()
            .map(|e| (e.kind.clone(), e.from, e.to))
            .collect();
        Self {
            schema,
            nodes,
            edges,
            by_id,
            by_name,
            by_seq,
            next_seq_by_kind,
            edge_keys,
        }
    }

    /// Folds every event given that belongs to `schema`'s map, in the
    /// order given, which must be log order. A fold reads exactly what
    /// it is given and knows nothing of paths: a caller after one
    /// path's map filters first. Events for other maps or of other
    /// kinds are skipped. An event that breaks a rule is an error naming it,
    /// not skipped: silently dropping it would hide that something
    /// went wrong at write time.
    pub fn fold<'a>(
        schema: impl Into<Arc<Schema>>,
        events: impl IntoIterator<Item = &'a Event>,
    ) -> Result<Self, MapError> {
        let schema = schema.into();
        let mut map = Self::empty(schema.clone());
        for event in events {
            if map_of(event.payload()) != Some(schema.name.as_str()) {
                continue;
            }
            map.replay(event.payload(), event.actor(), event.created_at())
                .map_err(|error| MapError::Rejected {
                    event: event.id(),
                    error: Box::new(error),
                })?;
        }
        Ok(map)
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The nodes of the schema's headline kinds, in map order - what a
    /// reader sees of the map before opening it.
    pub fn headlines(&self) -> impl Iterator<Item = &Node> {
        let headline_kinds = &self.schema.headline_kinds;
        self.nodes
            .iter()
            .filter(move |node| headline_kinds.contains(&node.kind))
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The nodes across every edge of `edge_kind` touching `id`, in map
    /// order, for a caller that knows no kind's name. `EdgeEnd::From`
    /// reads the `to` end of an edge whose `from` is `id`; `EdgeEnd::To`
    /// reads the `from` end of an edge whose `to` is `id`.
    pub fn linked(&self, id: NodeId, edge_kind: &str, end: EdgeEnd) -> Vec<&Node> {
        self.edges
            .iter()
            .filter(|edge| edge.kind == edge_kind)
            .filter_map(|edge| match end {
                EdgeEnd::From if edge.from == id => self.node(edge.to),
                EdgeEnd::To if edge.to == id => self.node(edge.from),
                _ => None,
            })
            .collect()
    }

    /// When the map last gained or changed a node, or gained an edge;
    /// `None` while it is empty. A removal leaves no trace here - what
    /// was removed lives only in the events.
    pub fn last_changed(&self) -> Option<Timestamp> {
        let nodes = self.nodes.iter().map(|node| node.changed().at);
        let edges = self.edges.iter().map(|edge| edge.changed().at);
        nodes.chain(edges).max()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.by_id.get(&id).map(|&i| &self.nodes[i])
    }

    pub fn find(&self, kind: &str, name: &str) -> Option<&Node> {
        let id = self.by_name.get(&(kind.to_string(), name.to_string()))?;
        self.node(*id)
    }

    /// The next short id number for a fresh `kind` node. `apply` mints
    /// with this; `replay` falls back to it only for a `NodeAdded`
    /// whose event carried no `seq` of its own. Tracked in
    /// `next_seq_by_kind`, never by counting live nodes: a `NodeRemoved`
    /// must not free a number for reuse, or an old citation of it would
    /// point at whatever node minted it next.
    fn next_seq(&self, kind: &str) -> u32 {
        self.next_seq_by_kind.get(kind).copied().unwrap_or(1)
    }

    /// `id`'s short id - its kind's prefix and the number minted for it,
    /// `d41`. `None` when the map holds no such node.
    pub fn short_id(&self, id: NodeId) -> Option<String> {
        let node = self.node(id)?;
        let kind = self.schema.node_kind(&node.kind)?;
        Some(format!("{}{}", kind.prefix, node.seq))
    }

    /// Resolves `s` to a node id, either way a writer may name one:
    /// `kind:name`, or the short id this map's own render shows it as.
    /// A short id never contains `:`, so the two forms cannot collide.
    pub fn resolve_str(&self, s: &str) -> Result<NodeId, MapError> {
        match s.split_once(':') {
            Some((kind, name)) => self.resolve(NodeRef {
                kind: kind.to_string(),
                name: name.to_string(),
            }),
            None => self.resolve_short_id(s),
        }
    }

    /// `resolve_str`'s short-id half: `s` split at its first digit into
    /// a node kind's prefix and a sequence number, looked up in
    /// `by_seq`. Anything that doesn't take that shape, names no
    /// kind's prefix, or names a number the map never minted is the
    /// one error - a short id has nothing else to suggest instead of.
    fn resolve_short_id(&self, s: &str) -> Result<NodeId, MapError> {
        let unknown = || MapError::UnknownShortId(s.to_string());
        let at = s.find(|c: char| c.is_ascii_digit()).filter(|&at| at > 0);
        let (prefix, digits) = at.map(|at| s.split_at(at)).ok_or_else(unknown)?;
        if !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(unknown());
        }
        let kind = self
            .schema
            .node_kinds
            .iter()
            .find(|kind| kind.prefix == prefix)
            .ok_or_else(unknown)?;
        let seq: u32 = digits.parse().map_err(|_| unknown())?;
        self.by_seq
            .get(&(kind.kind.clone(), seq))
            .copied()
            .ok_or_else(unknown)
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

    /// The map cut to `node` and every node within `depth` edges of
    /// it, following edges both ways: what is around a file is what it
    /// imports and what imports it. Depth zero is the node alone.
    pub fn around(&self, node: &NodeRef, depth: usize) -> Result<Self, MapError> {
        let start = self.resolve(node.clone())?;
        let mut reached: HashSet<NodeId> = HashSet::from([start]);
        let mut frontier: HashSet<NodeId> = HashSet::from([start]);
        for _ in 0..depth {
            let mut next = HashSet::new();
            for edge in &self.edges {
                for (near, far) in [(edge.from, edge.to), (edge.to, edge.from)] {
                    if frontier.contains(&near) && reached.insert(far) {
                        next.insert(far);
                    }
                }
            }
            frontier = next;
        }
        let nodes = self
            .nodes
            .iter()
            .filter(|node| reached.contains(&node.id))
            .cloned()
            .collect();
        Ok(self.cut_to(nodes))
    }

    /// The map cut to what it gained since `at`: the nodes added or
    /// changed then or later, plus the ends of every edge added then or
    /// later, so a new decision resolving an old question shows the
    /// question too. Edges from before `at` are not in the cut, even
    /// between kept nodes - they are not what changed.
    pub fn since(&self, at: Timestamp) -> Self {
        let fresh: Vec<&Edge> = self
            .edges
            .iter()
            .filter(|edge| edge.changed().at >= at)
            .collect();
        let touched: HashSet<NodeId> = fresh.iter().flat_map(|edge| [edge.from, edge.to]).collect();
        let nodes = self
            .nodes
            .iter()
            .filter(|node| node.changed().at >= at || touched.contains(&node.id))
            .cloned()
            .collect();
        let edges = fresh.into_iter().cloned().collect();
        Self::from_parts(self.schema.clone(), nodes, edges)
    }

    /// The map cut to `selection`, in its fixed order, counting what
    /// the cut left out. Consumes the map: a whole selection is the map
    /// itself, not a copy.
    pub fn select(self, selection: &Selection) -> Result<Fragment, MapError> {
        let total_nodes = self.nodes.len();
        let total_edges = self.edges.len();
        // An empty map has nothing to cut, and a node it lacks is not an
        // error to report over "nothing recorded yet".
        if selection.is_whole() || self.nodes.is_empty() {
            return Ok(Fragment {
                map: self,
                total_nodes,
                total_edges,
                boundary_edges: 0,
            });
        }
        let mut cut = match selection.around {
            Some((node, depth)) => self.around(node, depth)?,
            None => self.cut_to(self.nodes.clone()),
        };
        if let Some(at) = selection.since {
            cut = cut.since(at);
        }
        if !selection.kinds.is_empty() {
            cut = cut.keep_kinds(selection.kinds)?;
        }
        let boundary_edges = self
            .edges
            .iter()
            .filter(|edge| cut.node(edge.from).is_some() != cut.node(edge.to).is_some())
            .count();
        Ok(Fragment {
            map: cut,
            total_nodes,
            total_edges,
            boundary_edges,
        })
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
        Self::from_parts(self.schema.clone(), nodes, edges)
    }
}

/// The map as text for a model to read: one line per node, then one
/// per edge, nodes named by kind and name - the way a writer refers to
/// them - never by id. Names and property values are quoted, so a
/// newline inside one stays inside its line. Empty for an empty map.
impl fmt::Display for Map {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for node in &self.nodes {
            writeln!(f, "- {node}{}", node.properties_line())?;
        }
        for edge in &self.edges {
            writeln!(f, "- {}", self.edge_line(edge))?;
        }
        Ok(())
    }
}

impl Node {
    /// The tail of a node's line, wherever one is printed: `: key:
    /// "value"; key: "value"` over its properties, empty when it has
    /// none. Values are quoted, so a newline inside one stays inside
    /// its line.
    pub fn properties_line(&self) -> String {
        let mut out = String::new();
        let mut sep = ": ";
        for (key, value) in &self.properties {
            let _ = write!(out, "{sep}{key}: {value:?}");
            sep = "; ";
        }
        out
    }
}

impl Map {
    /// An edge as a line - `from kind to`, each end named the way a
    /// writer refers to it.
    pub fn edge_line(&self, edge: &Edge) -> String {
        format!(
            "{} {} {}",
            self.label(edge.from),
            edge.kind,
            self.label(edge.to)
        )
    }
}

/// A name split into the parts that make node names comparable: path
/// components, `::`-separated symbol parts, and words. Empty parts drop
/// out, so `src/providers/mod.rs` yields `src`, `providers`, `mod.rs`.
fn segments(name: &str) -> impl Iterator<Item = &str> {
    name.split(|c: char| c == '/' || c == ':' || c.is_whitespace())
        .filter(|part| !part.is_empty())
}

/// Whether a name reads as a path or a symbol rather than prose - a
/// `/` or a `::` in it - so a single shared segment is a real hint.
fn is_path_like(name: &str) -> bool {
    name.contains('/') || name.contains("::")
}

/// Which map a payload changes, if it changes one.
pub fn map_of(payload: &Payload) -> Option<&str> {
    match payload {
        Payload::NodeAdded { map, .. }
        | Payload::NodeChanged { map, .. }
        | Payload::NodeRemoved { map, .. }
        | Payload::EdgeAdded { map, .. }
        | Payload::EdgeRemoved { map, .. } => Some(map),
        _ => None,
    }
}


#[cfg(test)]
mod tests;
