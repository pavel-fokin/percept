//! A cognitive map: a graph the model builds from the log and reasons
//! over. A `Schema` says which node and edge kinds a map allows; a
//! `Map` is folded from the map events in the log, so the log stays
//! the one source of truth and a map is a view a reader rebuilds.
//! `Map::apply` turns a `Mutation` into the `Payload` that records it.
//! Every writer - the CLI, the model's tool - goes through `apply`, so
//! one place holds the rules.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::sync::Arc;

use super::{Actor, Event, EventId, Payload};
use crate::shared::{Id, Timestamp};

/// Which node and edge kinds a map allows. Data, not an enum: adding a
/// map is adding a value.
#[derive(Debug, PartialEq, Eq)]
pub struct Schema {
    pub name: String,
    /// The one reasoning operation this map makes cheap, as a reader
    /// deciding whether to open it needs to hear it - what the prompt
    /// carries in place of the map.
    pub purpose: String,
    pub node_kinds: Vec<NodeKind>,
    pub edge_kinds: Vec<EdgeKind>,
    /// The node kinds worth a reader's attention without opening the
    /// whole map - what `MapShape::Headlines` sends.
    pub headline_kinds: Vec<String>,
}

/// A node kind and, when its name does not say it all, one line saying
/// what it is, so a reader who meets the kind in a map's output learns
/// its meaning without a separate doc. The gloss lives here and
/// nowhere else.
#[derive(Debug, PartialEq, Eq)]
pub struct NodeKind {
    pub kind: String,
    pub gloss: String,
    /// The properties a new node of this kind must carry - `why` on an
    /// option or a task. Checked on a write, never on a fold, so what
    /// was recorded before the rule still folds.
    pub requires: Vec<String>,
    /// This node kind's short id prefix - `d` for `decision`, so a
    /// node reads as `d41` rather than its full id.
    pub prefix: String,
    /// The values a `state` property on a node of this kind may hold -
    /// `["open", "done", "dropped"]` on `task`, a set with no value
    /// open by position. Empty when the kind carries no state at all.
    /// Checked on a write, never on a fold, the way `requires` is.
    pub states: Vec<String>,
}

/// A kind's prefix when its schema names none: the name's own first
/// character, lowercased - `d` for `decision`, `t` for `task`. Used
/// both by `NodeKind::new` and by the TOML loader, so the one rule for
/// "no prefix given" lives once.
pub fn default_prefix(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_lowercase().to_string())
        .unwrap_or_default()
}

impl NodeKind {
    pub(crate) fn new(kind: &str, gloss: &str) -> Self {
        Self {
            prefix: default_prefix(kind),
            kind: kind.to_string(),
            gloss: gloss.to_string(),
            requires: Vec::new(),
            states: Vec::new(),
        }
    }

    /// `self`, requiring `property` on a new node of this kind. Used
    /// only by `core::testing`'s fixture schemas today, so it is
    /// `cfg(test)` like the rest of that module.
    #[cfg(test)]
    pub(crate) fn requiring(mut self, property: &str) -> Self {
        self.requires.push(property.to_string());
        self
    }

    /// `self`, with `states` as the values a `state` property on a node
    /// of this kind may hold. Used only by `core::testing`'s fixture
    /// schemas, so `cfg(test)` too.
    #[cfg(test)]
    pub(crate) fn with_states(mut self, states: &[&str]) -> Self {
        self.states = states.iter().map(|s| s.to_string()).collect();
        self
    }

    /// This kind's name, backticked, alone or with the properties it
    /// requires - `` `option` (requires `why`) `` - for a csv or a
    /// rendered list, so the one shape is built once and read
    /// everywhere a kind is named.
    pub fn label(&self) -> String {
        if self.requires.is_empty() {
            format!("`{}`", self.kind)
        } else {
            let requires: Vec<String> = self.requires.iter().map(|p| format!("`{p}`")).collect();
            format!("`{}` (requires {})", self.kind, requires.join(", "))
        }
    }
}

/// An edge kind and one line saying what it is, plus the node kinds it
/// may join: `from` on the tail, `to` on the head, each naming one or
/// more node kinds by name. `Map::apply` refuses an `AddEdge` whose
/// ends are not of these kinds.
#[derive(Debug, PartialEq, Eq)]
pub struct EdgeKind {
    pub kind: String,
    pub gloss: String,
    pub from: Vec<String>,
    pub to: Vec<String>,
}

impl EdgeKind {
    pub(crate) fn new(kind: &str, gloss: &str, from: &[&str], to: &[&str]) -> Self {
        Self {
            kind: kind.to_string(),
            gloss: gloss.to_string(),
            from: from.iter().map(|s| s.to_string()).collect(),
            to: to.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// This kind's name, backticked, with its ends - `` `contains`
    /// (file -> function | type) `` - so a reader meets the direction
    /// alongside the gloss.
    pub fn label(&self) -> String {
        format!(
            "`{}` ({} -> {})",
            self.kind,
            self.from.join(" | "),
            self.to.join(" | ")
        )
    }
}

/// The schemas a project has: every one, folded from the log, in one
/// list. Built once at the entrypoint from the project's own TOML
/// files; every fold, write, and error message goes through this, so
/// no caller keeps its own list.
pub struct Schemas {
    schemas: Vec<Arc<Schema>>,
}

impl Schemas {
    /// `folded`, each wrapped in `Arc` - the one full set every caller
    /// builds from.
    pub fn new(folded: Vec<Schema>) -> Self {
        let schemas: Vec<Arc<Schema>> = folded.into_iter().map(Arc::new).collect();
        Self { schemas }
    }

    /// The schema `name` names, or the error every boundary that folds
    /// or writes a map by name reports.
    pub fn find(&self, name: &str) -> Result<Arc<Schema>, MapError> {
        self.schemas
            .iter()
            .find(|schema| schema.name == name)
            .cloned()
            .ok_or_else(|| MapError::UnknownMap {
                name: name.to_string(),
                maps: self.names_csv(),
            })
    }

    /// Every schema, in stored order.
    pub fn folded(&self) -> impl Iterator<Item = &Arc<Schema>> + '_ {
        self.schemas.iter()
    }

    /// Every map `Map::fold` gives for `events` - one per schema.
    pub fn fold_all<'a>(
        &self,
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Map>, MapError> {
        self.folded()
            .map(|schema| Map::fold(schema.clone(), events.clone()))
            .collect()
    }

    /// Every schema's name, in stored order, for an "expected one of"
    /// error.
    fn names_csv(&self) -> String {
        self.schemas
            .iter()
            .map(|schema| schema.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Schema {
    /// The node kind `name` names, when the schema has it.
    pub fn node_kind(&self, name: &str) -> Option<&NodeKind> {
        self.node_kinds.iter().find(|k| k.kind == name)
    }

    /// The edge kind `name` names, when the schema has it.
    pub fn edge_kind(&self, name: &str) -> Option<&EdgeKind> {
        self.edge_kinds.iter().find(|k| k.kind == name)
    }

    /// The node kind names, in schema order.
    pub fn node_kind_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.node_kinds.iter().map(|k| k.kind.as_str())
    }

    /// The edge kind names, in schema order.
    pub fn edge_kind_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.edge_kinds.iter().map(|k| k.kind.as_str())
    }

    /// The node kinds' labels as a `, `-joined list, for a prompt line
    /// or an "expected one of" error.
    pub fn node_kinds_csv(&self) -> String {
        self.node_kinds
            .iter()
            .map(NodeKind::label)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The edge kinds' labels as a `, `-joined list.
    pub fn edge_kinds_csv(&self) -> String {
        self.edge_kinds
            .iter()
            .map(EdgeKind::label)
            .collect::<Vec<_>>()
            .join(", ")
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

/// Why a mutation, or a stored event, doesn't fit its map. Each names
/// the rule and the value that broke it. `apply` checks a mutation
/// before it becomes an event, so a stored event that breaks a rule
/// means a race between writers or a hand-edited log. Edge ends are
/// carried as labels - kind and name - the way a reader knows them.
#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    UnknownMap {
        name: String,
        /// Every map `Schemas` knows, `, `-joined - what the error
        /// offers in place of a global registry.
        maps: String,
    },
    UnknownNodeKind {
        map: String,
        kinds: String,
        kind: String,
    },
    UnknownEdgeKind {
        map: String,
        kinds: String,
        kind: String,
    },
    /// A name that is blank would be a node nobody can point at.
    BlankName,
    /// A new node of a kind that `requires` a property the caller did
    /// not supply. A rule for new writes only, so a node recorded
    /// before its kind gained the requirement still folds; checked by
    /// `Map::apply` on a mutation, never by `replay` on a stored
    /// payload.
    MissingProperty {
        kind: String,
        name: String,
        property: String,
        gloss: String,
    },
    DuplicateNode {
        kind: String,
        name: String,
    },
    /// A `state` property whose value is not among the values its
    /// node's kind declares - including a kind that declares none at
    /// all, whose list is then empty. Write-only: checked by
    /// `Map::apply` on `AddNode` and `ChangeNode`, never by `replay`.
    UnknownState {
        kind: String,
        value: String,
        states: Vec<String>,
    },
    /// A new node of a kind that declares states, sent without one.
    /// Write-only, like `UnknownState`.
    MissingState {
        kind: String,
        states: Vec<String>,
    },
    /// A rename, a property other than `state`, or a removal that W6's
    /// rank rule refuses: the actor neither owns nor outranks the
    /// writer, or is outranked by whoever touched it since.
    /// Write-only: `state` and a new edge are any actor's.
    NotYours {
        node: String,
        owner: Actor,
        touched_by: Actor,
    },
    /// A `why` that is given but blank - on a change, a removal, or an
    /// edge removal. Write-only.
    BlankWhy,
    NoSuchNode {
        node: NodeRef,
        /// Nodes of the same kind whose name overlaps `node.name`, as
        /// `kind:name` a reader can paste back. Empty when none do.
        suggestions: Vec<String>,
    },
    /// A stored event names a node id the fold never saw.
    NoSuchNodeId(NodeId),
    /// A short id that names no node kind's prefix, or a number the map
    /// never minted for it.
    UnknownShortId(String),
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
    /// A new edge whose `from` or `to` end is not of a kind its edge
    /// kind allows. A write-only rule: `replay` never checks ends, so
    /// an edge recorded before its kind declared ends still folds.
    WrongEdgeEnd {
        edge_kind: String,
        end: EdgeEnd,
        allowed: Vec<String>,
        found: String,
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
            Self::UnknownMap { name, maps } if maps.is_empty() => {
                write!(f, "no map named {name:?}; no map is declared")
            }
            Self::UnknownMap { name, maps } => write!(f, "no map named {name:?}; maps are {maps}"),
            Self::UnknownNodeKind { map, kinds, kind } => write!(
                f,
                "no node kind {kind:?} in map {map:?}; kinds are {kinds}"
            ),
            Self::UnknownEdgeKind { map, kinds, kind } => write!(
                f,
                "no edge kind {kind:?} in map {map:?}; kinds are {kinds}"
            ),
            Self::BlankName => write!(f, "a node's name must not be blank"),
            Self::BlankWhy => write!(f, "a why must not be blank"),
            Self::MissingProperty {
                kind,
                name,
                property,
                gloss,
            } => {
                write!(f, "{kind} {name:?} lacks its `{property}` property, which every {kind} carries")?;
                if !gloss.is_empty() {
                    write!(f, ": {gloss}")?;
                }
                Ok(())
            }
            Self::DuplicateNode { kind, name } => {
                write!(f, "{kind} {name:?} is already in the map")
            }
            Self::MissingState { kind, states } => {
                write!(f, "{kind} needs a state; states are {}", states.join(", "))
            }
            Self::UnknownState { kind, value, states } => write!(
                f,
                "{kind} has no state {value:?}; states are {}",
                if states.is_empty() {
                    "none - this kind carries no state".to_string()
                } else {
                    states.join(", ")
                }
            ),
            Self::NotYours {
                node,
                owner,
                touched_by,
            } => write!(
                f,
                "{node} was written by {} and touched by {}; you may still set a node's state, \
                 or add a node and an edge beside it",
                owner.name(),
                touched_by.name()
            ),
            Self::NoSuchNode { node, suggestions } => {
                write!(f, "no {node} in the map")?;
                if !suggestions.is_empty() {
                    write!(f, "; did you mean {}", suggestions.join(", "))?;
                }
                Ok(())
            }
            Self::NoSuchNodeId(id) => write!(f, "no node with id {}", id.as_uuid()),
            Self::UnknownShortId(s) => write!(f, "no node with the short id {s:?}"),
            Self::DuplicateEdge { kind, from, to } => {
                write!(f, "{from} {kind} {to} is already in the map")
            }
            Self::NoSuchEdge { kind, from, to } => write!(f, "no edge {from} {kind} {to}"),
            Self::WrongEdgeEnd {
                edge_kind,
                end,
                allowed,
                found,
            } => write!(
                f,
                "a {edge_kind:?} edge's {end} end must be {}; found {found}",
                allowed.join(" or ")
            ),
            Self::Rejected { event, error } => {
                write!(f, "event {} does not fit its map: {error}", event.as_uuid())
            }
        }
    }
}

impl std::error::Error for MapError {}

/// An actor's rank: `Human` above `Agent` and `System`, which sit
/// level with each other. What W6's `may` weighs - not a property
/// `Actor` itself carries, since ranking is a rule of the map's rank
/// lock, not something every reader of an actor needs.
fn rank(actor: Actor) -> u8 {
    match actor {
        Actor::Human(_) => 2,
        Actor::Agent | Actor::System => 1,
    }
}

/// Whether `a` outranks `b` - strictly above it, never level.
fn outranks(a: Actor, b: Actor) -> bool {
    rank(a) > rank(b)
}

/// Whether `a` and `b` are the one who wrote something. A human equals
/// any human: two humans are not told apart yet.
fn same_actor(a: Actor, b: Actor) -> bool {
    matches!(
        (a, b),
        (Actor::Human(_), Actor::Human(_)) | (Actor::Agent, Actor::Agent) | (Actor::System, Actor::System)
    )
}

/// W6's rank rule: whether `actor` may rename, change a property beyond
/// `state`, or remove something `owner` wrote and `touched_by` is the
/// highest rank to have touched. `actor` must own it or outrank
/// `owner`, and must not be outranked by `touched_by` - a change from
/// above locks what it touched against everyone below that rank.
fn may(actor: Actor, owner: Actor, touched_by: Actor) -> bool {
    (same_actor(actor, owner) || outranks(actor, owner)) && !outranks(touched_by, actor)
}

/// The highest-ranked of `actors`; `None` when there are none.
fn highest(actors: impl Iterator<Item = Actor>) -> Option<Actor> {
    actors.max_by_key(|actor| rank(*actor))
}

/// Whether `why`, when given, is blank - W2's `BlankWhy`.
fn blank_why(why: Option<&str>) -> Result<(), MapError> {
    match why {
        Some(why) if why.trim().is_empty() => Err(MapError::BlankWhy),
        _ => Ok(()),
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

    /// Checks `mutation` against the schema and the map's current
    /// state, applies it, and returns the `Payload` that records it.
    /// The caller commits that payload; the map is already updated, so
    /// a batch can check each step against the ones before it.
    pub fn apply(&mut self, mutation: Mutation, actor: Actor) -> Result<Payload, MapError> {
        let map = self.schema.name.to_string();
        let payload = match mutation {
            Mutation::AddNode {
                kind,
                name,
                properties,
                sources,
            } => {
                if let Some(node_kind) = self.schema.node_kind(&kind) {
                    if let Some(property) = node_kind
                        .requires
                        .iter()
                        .find(|property| !properties.contains_key(*property))
                    {
                        return Err(MapError::MissingProperty {
                            kind: kind.clone(),
                            name: name.clone(),
                            property: property.clone(),
                            gloss: node_kind.gloss.clone(),
                        });
                    }
                    if !node_kind.states.is_empty() && !properties.contains_key("state") {
                        return Err(MapError::MissingState {
                            kind: kind.clone(),
                            states: node_kind.states.clone(),
                        });
                    }
                    check_state(node_kind, &properties)?;
                }
                Payload::NodeAdded {
                    map,
                    node: NodeId::new(),
                    seq: self.next_seq(&kind),
                    kind,
                    name,
                    properties,
                    sources,
                }
            }
            Mutation::ChangeNode {
                node,
                name,
                properties,
                sources,
                why,
            } => {
                let node_id = self.resolve(node)?;
                let existing = self.node(node_id).expect("resolve returns a live node's id");
                if let Some(node_kind) = self.schema.node_kind(&existing.kind) {
                    check_state(node_kind, &properties)?;
                }
                blank_why(why.as_deref())?;
                let needs_rank = name.is_some() || properties.keys().any(|key| key != "state");
                if needs_rank {
                    self.check_may_of(actor, self.label(node_id), existing)?;
                }
                Payload::NodeChanged {
                    map,
                    node: node_id,
                    name,
                    properties,
                    sources,
                    why,
                }
            }
            Mutation::RemoveNode { node, why, sources } => {
                let node_id = self.resolve(node)?;
                blank_why(Some(&why))?;
                let existing = self.node(node_id).expect("resolve returns a live node's id");
                self.check_may_of(actor, self.label(node_id), existing)?;
                // Removing a node drops every edge on it, so each one is
                // weighed as its own removal would be.
                for edge in self.edges.iter().filter(|edge| edge.from == node_id || edge.to == node_id) {
                    self.check_may_remove_edge(actor, edge)?;
                }
                Payload::NodeRemoved {
                    map,
                    node: node_id,
                    why,
                    sources,
                }
            }
            Mutation::AddEdge {
                kind,
                from,
                to,
                sources,
            } => {
                let from_id = self.resolve(from)?;
                let to_id = self.resolve(to)?;
                self.check_edge_ends(&kind, from_id, to_id)?;
                Payload::EdgeAdded {
                    map,
                    kind,
                    from: from_id,
                    to: to_id,
                    sources,
                }
            }
            Mutation::RemoveEdge {
                kind,
                from,
                to,
                sources,
                why,
            } => {
                let from_id = self.resolve(from)?;
                let to_id = self.resolve(to)?;
                blank_why(Some(&why))?;
                if let Some(edge) = self.find_edge(&kind, from_id, to_id) {
                    self.check_may_remove_edge(actor, edge)?;
                }
                Payload::EdgeRemoved {
                    map,
                    kind,
                    from: from_id,
                    to: to_id,
                    sources,
                    why,
                }
            }
        };
        self.replay(&payload, actor, Timestamp::now())?;
        Ok(payload)
    }

    /// Applies one recorded change - every rule a map enforces lives
    /// here, so a fold and `apply` agree. Removing a node drops the
    /// edges that touch it: an edge to nothing is not a fact. `actor`
    /// and `at` stamp a node or edge this call adds - who and when,
    /// from the event that carried it.
    fn replay(&mut self, payload: &Payload, actor: Actor, at: Timestamp) -> Result<(), MapError> {
        match payload {
            Payload::NodeAdded {
                node,
                kind,
                name,
                properties,
                sources,
                seq,
                ..
            } => {
                self.check_node_kind(kind)?;
                self.check_name(kind, name, None)?;
                // `0` is an event from before short ids existed - the
                // same count `apply` would have minted for it, taken
                // here from its position among nodes of its kind, since
                // nothing recorded one at the time.
                let seq = if *seq == 0 { self.next_seq(kind) } else { *seq };
                let next = self.next_seq_by_kind.entry(kind.clone()).or_insert(1);
                *next = (*next).max(seq + 1);
                self.by_id.insert(*node, self.nodes.len());
                self.by_name.insert((kind.clone(), name.clone()), *node);
                self.by_seq.insert((kind.clone(), seq), *node);
                self.nodes.push(Node {
                    id: *node,
                    kind: kind.clone(),
                    name: name.clone(),
                    properties: properties.clone(),
                    sources: sources.clone(),
                    history: vec![Change { actor, at, why: None }],
                    seq,
                });
            }
            Payload::NodeChanged {
                node,
                name,
                properties,
                sources,
                why,
                ..
            } => {
                let index = *self.by_id.get(node).ok_or(MapError::NoSuchNodeId(*node))?;
                let kind = self.nodes[index].kind.clone();
                if let Some(new_name) = name {
                    self.check_name(&kind, new_name, Some(*node))?;
                    let old_name = self.nodes[index].name.clone();
                    self.by_name.remove(&(kind.clone(), old_name));
                    self.by_name.insert((kind.clone(), new_name.clone()), *node);
                    self.nodes[index].name = new_name.clone();
                }
                for (key, value) in properties {
                    self.nodes[index].properties.insert(key.clone(), value.clone());
                }
                for source in sources {
                    if !self.nodes[index].sources.contains(source) {
                        self.nodes[index].sources.push(*source);
                    }
                }
                self.nodes[index].history.push(Change {
                    actor,
                    at,
                    why: why.clone(),
                });
            }
            Payload::NodeRemoved { node, .. } => {
                let removed = self.node(*node).ok_or(MapError::NoSuchNodeId(*node))?;
                let key = (removed.kind.clone(), removed.name.clone());
                let seq_key = (removed.kind.clone(), removed.seq);
                self.by_name.remove(&key);
                self.by_seq.remove(&seq_key);
                self.nodes.retain(|n| n.id != *node);
                self.by_id = self
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.id, i))
                    .collect();
                self.edges.retain(|e| e.from != *node && e.to != *node);
                self.edge_keys
                    .retain(|(_, from, to)| from != node && to != node);
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
                if !self.edge_keys.insert((kind.clone(), *from, *to)) {
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
                    history: vec![Change { actor, at, why: None }],
                });
            }
            Payload::EdgeRemoved { kind, from, to, .. } => {
                if !self.edge_keys.remove(&(kind.clone(), *from, *to)) {
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
        match self.find(&node.kind, &node.name) {
            Some(n) => Ok(n.id),
            None => {
                let suggestions = self.suggestions_for(&node);
                Err(MapError::NoSuchNode { node, suggestions })
            }
        }
    }

    /// Up to five nodes to retry with, each named `kind:name`, most
    /// name-overlap first. The same-kind pass wants two shared `/`,
    /// `::`, or whitespace segments: one shared word is noise in a
    /// prose name - every `decision` shares "the" - and a weak hint in
    /// a path. When it finds nothing, a cross-kind pass runs, since a
    /// wrong kind is how `package:providers` misses a real
    /// `file:src/providers/...`; there one shared segment is enough,
    /// but only when a name has a `/` or `::` in it, so a path or
    /// symbol lookup crosses kinds while a prose one stays silent.
    fn suggestions_for(&self, node: &NodeRef) -> Vec<String> {
        let same_kind =
            self.suggestions_matching(node, |n| n.kind == node.kind, |_, shared| shared >= 2);
        if !same_kind.is_empty() {
            return same_kind;
        }
        let query_is_path = is_path_like(&node.name);
        self.suggestions_matching(
            node,
            |_| true,
            |candidate, shared| {
                shared >= 2 || (shared == 1 && (query_is_path || is_path_like(candidate)))
            },
        )
    }

    /// Nodes `keep` admits whose name shares segments with `node`'s
    /// that `strong` accepts, given the candidate's name and the count
    /// shared; scored by overlap, five at most, each named `kind:name`.
    fn suggestions_matching(
        &self,
        node: &NodeRef,
        keep: impl Fn(&Node) -> bool,
        strong: impl Fn(&str, usize) -> bool,
    ) -> Vec<String> {
        let wanted: HashSet<&str> = segments(&node.name).collect();
        let mut scored: Vec<(usize, &Node)> = self
            .nodes
            .iter()
            .filter(|n| keep(n))
            .filter_map(|n| {
                let have: HashSet<&str> = segments(&n.name).collect();
                let shared = wanted.intersection(&have).count();
                strong(&n.name, shared).then_some((shared, n))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        scored
            .into_iter()
            .take(5)
            .map(|(_, n)| format!("{}:{}", n.kind, n.name))
            .collect()
    }

    /// A node as `Display` names it; the id when the map has no such
    /// node.
    pub fn label(&self, id: NodeId) -> String {
        self.node(id)
            .map_or_else(|| id.as_uuid().to_string(), Node::to_string)
    }

    fn check_node_kind(&self, kind: &str) -> Result<(), MapError> {
        if self.schema.node_kind_names().any(|name| name == kind) {
            Ok(())
        } else {
            Err(MapError::UnknownNodeKind {
                map: self.schema.name.clone(),
                kinds: self.schema.node_kinds_csv(),
                kind: kind.to_string(),
            })
        }
    }

    fn check_edge_kind(&self, kind: &str) -> Result<(), MapError> {
        if self.schema.edge_kind_names().any(|name| name == kind) {
            Ok(())
        } else {
            Err(MapError::UnknownEdgeKind {
                map: self.schema.name.clone(),
                kinds: self.schema.edge_kinds_csv(),
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

    /// W6 as a refusal: `NotYours` naming `label` unless `may` holds.
    fn check_may(&self, actor: Actor, label: String, owner: Actor, touched_by: Actor) -> Result<(), MapError> {
        if may(actor, owner, touched_by) {
            Ok(())
        } else {
            Err(MapError::NotYours {
                node: label,
                owner,
                touched_by,
            })
        }
    }

    /// `check_may` for `node` itself, where nothing else weighs in.
    fn check_may_of(&self, actor: Actor, label: String, node: &Node) -> Result<(), MapError> {
        self.check_may(actor, label, node.added().actor, node.touched_by())
    }

    /// W6 for dropping `edge`, whether on its own or with a node it
    /// hangs on. An edge is part of both ends' neighbourhoods, so the
    /// highest rank to have touched either end, or the edge itself, is
    /// what locks it.
    fn check_may_remove_edge(&self, actor: Actor, edge: &Edge) -> Result<(), MapError> {
        let ends = [edge.from, edge.to].map(|id| self.node(id).expect("an edge's ends are live"));
        let touched_by = highest(ends.iter().map(|end| end.touched_by()).chain([edge.touched_by()]))
            .expect("three candidates");
        self.check_may(actor, self.edge_line(edge), edge.added().actor, touched_by)
    }

    /// The live edge `kind from to` names, if the map holds one - what
    /// `RemoveEdge`'s W6 check weighs. `None` when the map holds no such
    /// edge; `replay` is what reports that missing.
    fn find_edge(&self, kind: &str, from: NodeId, to: NodeId) -> Option<&Edge> {
        self.edges
            .iter()
            .find(|edge| edge.kind == kind && edge.from == from && edge.to == to)
    }

    /// Refuses an `AddEdge` whose `from` or `to` node is not of a kind
    /// `kind` allows at that end. A kind `apply` has not yet checked
    /// against the schema is let through here; `check_edge_kind` in
    /// `replay` is what refuses it. A write-only rule, never run by
    /// `replay`, so an edge recorded before its kind declared ends
    /// still folds.
    fn check_edge_ends(&self, kind: &str, from: NodeId, to: NodeId) -> Result<(), MapError> {
        let Some(edge_kind) = self.schema.edge_kind(kind) else {
            return Ok(());
        };
        let ends = [
            (EdgeEnd::From, &edge_kind.from, from),
            (EdgeEnd::To, &edge_kind.to, to),
        ];
        for (end, allowed, id) in ends {
            let node = self.node(id).ok_or(MapError::NoSuchNodeId(id))?;
            if !allowed.contains(&node.kind) {
                return Err(MapError::WrongEdgeEnd {
                    edge_kind: kind.to_string(),
                    end,
                    allowed: allowed.clone(),
                    found: node.kind.clone(),
                });
            }
        }
        Ok(())
    }

    /// Refuses a blank `name`, or one another node of `kind` holds -
    /// `except` being the node itself when a rename keeps its name.
    /// The one rule `NodeAdded` and a `NodeChanged` rename share.
    fn check_name(&self, kind: &str, name: &str, except: Option<NodeId>) -> Result<(), MapError> {
        if name.trim().is_empty() {
            return Err(MapError::BlankName);
        }
        if self
            .find(kind, name)
            .is_some_and(|holder| Some(holder.id) != except)
        {
            return Err(MapError::DuplicateNode {
                kind: kind.to_string(),
                name: name.to_string(),
            });
        }
        Ok(())
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

/// Refuses a `state` property in `properties` whose value `kind` does
/// not list - including a kind with no `states` at all, whose list is
/// then empty. Shared by `Mutation::AddNode` and `Mutation::ChangeNode`
/// in `Map::apply`.
fn check_state(kind: &NodeKind, properties: &BTreeMap<String, String>) -> Result<(), MapError> {
    if let Some(value) = properties.get("state") {
        if !kind.states.iter().any(|state| state == value) {
            return Err(MapError::UnknownState {
                kind: kind.kind.clone(),
                value: value.clone(),
                states: kind.states.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
