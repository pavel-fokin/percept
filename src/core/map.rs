//! A cognitive map: a graph the model builds from the log and reasons
//! over. A `Schema` says which node and edge kinds a map allows; a
//! `Map` is folded from the map events in the log, so the log stays
//! the one source of truth and a map is a view a reader rebuilds.
//! `Map::apply` turns a `Mutation` into the `Payload` that records it.
//! Every writer - the CLI, the model's tool - goes through `apply`, so
//! one place holds the rules.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::path::PathBuf;
use std::sync::Arc;

use super::{Actor, Event, EventId, Payload, Source};
use crate::shared::{Id, Timestamp};

/// Which events a fold may draw from: one project's alone, or every
/// project's. The log is shared by every project that writes to it;
/// `Project` is the default a reader wants, `All` the escape hatch.
/// Never touches `Snapshot::resolve` - a node may cite an event from
/// any project, whichever map it ends up in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Project(PathBuf),
    All,
}

impl Scope {
    /// Whether `event` falls inside this scope.
    pub fn admits(&self, event: &Event) -> bool {
        match self {
            Self::Project(path) => event.source().path == *path,
            Self::All => true,
        }
    }
}

impl Source {
    /// The scope of this writer's own project - what every fold on its
    /// behalf reads.
    pub fn scope(&self) -> Scope {
        Scope::Project(self.path.clone())
    }
}

/// Which node and edge kinds a map allows. Data, not an enum: adding a
/// map is adding a value.
#[derive(Debug, PartialEq, Eq)]
pub struct Schema {
    pub name: String,
    /// The one reasoning operation this map makes cheap, as a reader
    /// deciding whether to open it needs to hear it - what the prompt
    /// carries in place of the map.
    pub purpose: String,
    pub node_kinds: Vec<Kind>,
    pub edge_kinds: Vec<Kind>,
    /// The node kinds worth a reader's attention without opening the
    /// whole map - what `MapShape::Headlines` sends.
    pub headline_kinds: Vec<String>,
    /// Which node kind settles which through a `resolves` edge - a
    /// decision a question, an outcome a task - when the map has such
    /// a pair. A `resolves` edge between other kinds settles nothing.
    pub settlement: Option<Settlement>,
}

/// A node or edge kind and one line saying what it is, so a reader who
/// meets the kind name in a map's output learns its meaning without a
/// separate doc. The gloss lives here, beside the name, and nowhere
/// else.
#[derive(Debug, PartialEq, Eq)]
pub struct Kind {
    pub name: String,
    pub gloss: String,
    /// The properties a new node of this kind must carry - `why` on an
    /// option or a task. Checked on a write, never on a fold, so what
    /// was recorded before the rule still folds.
    pub requires: Vec<String>,
    /// A node kind's short id prefix - `d` for `decision`, so a node
    /// reads as `d41` rather than its full id. Meaningless on an edge
    /// kind, which is never referenced by a short id; left at its
    /// default there and never shown.
    pub prefix: String,
}

/// A kind's prefix when its schema names none: the name's own first
/// character, lowercased - `d` for `decision`, `t` for `task`. Used
/// both by `Kind::new` and by the TOML loader, so the one rule for
/// "no prefix given" lives once.
pub fn default_prefix(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_lowercase().to_string())
        .unwrap_or_default()
}

impl Kind {
    #[cfg(any(test, feature = "lab"))]
    pub(crate) fn new(name: &str, gloss: &str) -> Self {
        Self {
            prefix: default_prefix(name),
            name: name.to_string(),
            gloss: gloss.to_string(),
            requires: Vec::new(),
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

    /// This kind's name, backticked, alone or with the properties it
    /// requires - `` `option` (requires `why`) `` - for a csv or a
    /// rendered list, so the one shape is built once and read
    /// everywhere a kind is named.
    pub fn label(&self) -> String {
        if self.requires.is_empty() {
            format!("`{}`", self.name)
        } else {
            let requires: Vec<String> = self.requires.iter().map(|p| format!("`{p}`")).collect();
            format!("`{}` (requires {})", self.name, requires.join(", "))
        }
    }
}

/// The two node kinds a `resolves` edge joins: `by` settles `of`.
#[derive(Debug, PartialEq, Eq)]
pub struct Settlement {
    pub by: String,
    pub of: String,
}

/// The edge kind that corrects a decision: from the new one to the one
/// it replaces. The old node stays, one hop away, and leaves the
/// headlines - a removal would take a reader's landmark with it.
pub const SUPERSEDES: &str = "supersedes";

/// The edge kind from a question to a decision it puts in doubt. The
/// decision stands until a successor supersedes it; the question is how
/// a model raises the doubt without rewriting what the human agreed.
pub const REOPENS: &str = "reopens";

/// The edge kind from a decision to the question it settles.
pub const RESOLVES: &str = "resolves";

/// The edge kind from an option to the question it was weighed for.
pub const ANSWERS: &str = "answers";

/// The edge kind from a task to the task it must land before.
pub const BLOCKS: &str = "blocks";

/// The one node kind `revise_map` never removes: a decision is corrected
/// by a successor with a `supersedes` edge, so it is public.
#[cfg(feature = "lab")]
pub const DECISION: &str = "decision";
/// An alternative that lost. The store refuses one that does not say
/// why, so it is public too.
pub const OPTION: &str = "option";

/// The schemas a project has: every one, folded from the log, in one
/// list. Built once at the entrypoint from the built-in schemas and a
/// project's own TOML files; every fold, write, and error message goes
/// through this, so no caller keeps its own list.
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

    /// Every map folded from `events` within `scope` - one per schema.
    pub fn fold_all<'a>(
        &self,
        scope: &Scope,
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Map>, MapError> {
        self.folded()
            .map(|schema| Map::fold(schema.clone(), scope, events.clone()))
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
    pub fn node_kind(&self, name: &str) -> Option<&Kind> {
        self.node_kinds.iter().find(|kind| kind.name == name)
    }

    /// The edge kind `name` names, when the schema has it.
    pub fn edge_kind(&self, name: &str) -> Option<&Kind> {
        self.edge_kinds.iter().find(|kind| kind.name == name)
    }

    /// The node kind names, in schema order.
    pub fn node_kind_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.node_kinds.iter().map(|kind| kind.name.as_str())
    }

    /// The edge kind names, in schema order.
    pub fn edge_kind_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.edge_kinds.iter().map(|kind| kind.name.as_str())
    }

    /// The node kinds' labels as a `, `-joined list, for a prompt line
    /// or an "expected one of" error.
    pub fn node_kinds_csv(&self) -> String {
        self.node_kinds
            .iter()
            .map(Kind::label)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The edge kinds' labels as a `, `-joined list.
    pub fn edge_kinds_csv(&self) -> String {
        self.edge_kinds
            .iter()
            .map(Kind::label)
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
    /// Who added this node - the actor its `node.added` event carried.
    pub actor: Actor,
    /// When this node was added - that event's `created_at`.
    pub added_at: Timestamp,
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
    /// Who added this edge - the actor its `edge.added` event carried.
    pub actor: Actor,
    /// When this edge was added - that event's `created_at`.
    pub added_at: Timestamp,
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
    /// Wraps any of the above with the event that broke the rule.
    Rejected {
        event: EventId,
        error: Box<MapError>,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::MissingProperty {
                kind,
                name,
                property,
                gloss,
            } => write!(
                f,
                "{kind} {name:?} lacks its `{property}` property, which every {kind} \
                 carries: {gloss}"
            ),
            Self::DuplicateNode { kind, name } => {
                write!(f, "{kind} {name:?} is already in the map")
            }
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
            Self::Rejected { event, error } => {
                write!(f, "event {} does not fit its map: {error}", event.as_uuid())
            }
        }
    }
}

impl std::error::Error for MapError {}

/// How the human has judged a model's claim, derived - never stored
/// directly - from the `claim.confirmed`, `claim.disputed`, and
/// `review.finished` events that name a node, latest by log order.
/// `Claimed` is a model's claim nobody has judged yet; a user-written
/// node has no standing at all (see `Map::standing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Claimed,
    Seen,
    Confirmed,
    Disputed,
}

impl fmt::Display for Standing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Claimed => "claimed",
            Self::Seen => "seen",
            Self::Confirmed => "confirmed",
            Self::Disputed => "disputed",
        })
    }
}

/// The latest `claim.confirmed`/`claim.disputed` naming a node, so
/// `Map::dispute` can hand back the why without walking the log again,
/// and `at` its event's `created_at`, so `Map::judged_since` can tell a
/// fresh judgment from an old one.
#[derive(Clone)]
struct Judgment {
    at: Timestamp,
    kind: JudgmentKind,
}

#[derive(Clone)]
enum JudgmentKind {
    Confirmed,
    Disputed(String),
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
    // The latest `claim.confirmed`/`claim.disputed` naming each node, in
    // log order - what `standing` and `dispute` read.
    judgments: HashMap<NodeId, Judgment>,
    // Nodes at least one `review.finished` has named - `Standing::Seen`
    // for one with no judgment yet.
    reviewed: HashSet<NodeId>,
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
            judgments: HashMap::new(),
            reviewed: HashSet::new(),
        }
    }

    /// Folds the events that belong to `schema`'s map and fall inside
    /// `scope`, in the order given, which must be log order. Events for
    /// other maps, of other kinds, or outside `scope` are skipped. An
    /// event that breaks a rule is an error naming it, not skipped:
    /// silently dropping it would hide that something went wrong at
    /// write time.
    pub fn fold<'a>(
        schema: impl Into<Arc<Schema>>,
        scope: &Scope,
        events: impl IntoIterator<Item = &'a Event>,
    ) -> Result<Self, MapError> {
        let schema = schema.into();
        let mut map = Self::empty(schema.clone());
        for event in events {
            if !scope.admits(event) {
                continue;
            }
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

    /// The nodes of the schema's headline kinds, in map order, less the
    /// superseded ones - what a reader sees of the map before opening
    /// it.
    pub fn headlines(&self) -> impl Iterator<Item = &Node> {
        let headline_kinds = &self.schema.headline_kinds;
        self.nodes
            .iter()
            .filter(move |node| headline_kinds.contains(&node.kind))
            .filter(|node| !self.is_superseded(node.id))
    }

    /// The node with a `supersedes` edge to `id`. Two nodes superseding
    /// one is a writer's mistake; the first in map order wins.
    fn superseded_by(&self, id: NodeId) -> Option<NodeId> {
        self.edges
            .iter()
            .find(|edge| edge.kind == SUPERSEDES && edge.to == id)
            .map(|edge| edge.from)
    }

    pub fn is_superseded(&self, id: NodeId) -> bool {
        self.superseded_by(id).is_some()
    }

    /// The nodes `id` supersedes, in map order.
    fn supersedes(&self, id: NodeId) -> impl Iterator<Item = &Node> {
        self.edges
            .iter()
            .filter(move |edge| edge.kind == SUPERSEDES && edge.from == id)
            .filter_map(|edge| self.node(edge.to))
    }

    /// The end of `id`'s supersession chain: `id` itself when nothing
    /// supersedes it, else the node that does, followed until one is
    /// current. A cycle stops at the node already seen.
    pub fn successor(&self, id: NodeId) -> NodeId {
        let mut seen = HashSet::from([id]);
        let mut current = id;
        while let Some(next) = self.superseded_by(current) {
            if !seen.insert(next) {
                break;
            }
            current = next;
        }
        current
    }

    /// Everything `id` supersedes, transitively, nearest first - the
    /// `was` lines under a decision.
    pub fn predecessors(&self, id: NodeId) -> Vec<&Node> {
        let mut seen = HashSet::from([id]);
        let mut out = Vec::new();
        let mut frontier = vec![id];
        while let Some(current) = frontier.pop() {
            for node in self.supersedes(current) {
                if seen.insert(node.id) {
                    out.push(node);
                    frontier.push(node.id);
                }
            }
        }
        out
    }

    /// The nodes that settle `question` now - the decisions of a
    /// question, the outcomes of a task: each with a `resolves` edge to
    /// it, followed to the end of its supersession chain, so a
    /// correction needs no new `resolves` edge. In map order, each
    /// once. A `resolves` edge between other kinds settles nothing.
    pub fn settled_by(&self, question: NodeId) -> Vec<&Node> {
        let mut out: Vec<&Node> = Vec::new();
        for edge in self.resolving_edges() {
            if edge.to != question {
                continue;
            }
            if let Some(current) = self.node(self.successor(edge.from)) {
                if !out.iter().any(|node| node.id == current.id) {
                    out.push(current);
                }
            }
        }
        out
    }

    /// The headline nodes of the schema's settled kind - `question` on
    /// `decisions`, `task` on `tasks` - that nothing settles yet.
    /// Empty on a map with no `Settlement`. Headlines also include the
    /// settling kind itself (`decision` is a headline on `decisions`
    /// too, so `--around` and the render can name it directly), and
    /// nothing ever settles a settling-kind node, so filtering to the
    /// settled kind first is what keeps every one of those out of this
    /// list.
    pub fn open(&self) -> impl Iterator<Item = &Node> {
        let of = self.schema.settlement.as_ref().map(|s| s.of.as_str());
        self.headlines()
            .filter(move |node| Some(node.kind.as_str()) == of)
            .filter(|node| self.settled_by(node.id).is_empty())
    }

    /// Whether `decision`, or a decision it supersedes, has a
    /// `resolves` edge to a question - so it belongs under one in a
    /// render rather than on its own.
    pub fn settles(&self, decision: NodeId) -> bool {
        let chain: HashSet<NodeId> = std::iter::once(decision)
            .chain(self.predecessors(decision).iter().map(|node| node.id))
            .collect();
        self.resolving_edges()
            .any(|edge| chain.contains(&edge.from))
    }

    /// The `resolves` edges that run between the schema's settlement
    /// kinds - none on a map without a settlement.
    fn resolving_edges(&self) -> impl Iterator<Item = &Edge> {
        let settlement = self.schema.settlement.as_ref();
        self.edges.iter().filter(move |edge| {
            let Some(Settlement { by, of }) = settlement else {
                return false;
            };
            edge.kind == RESOLVES
                && self.node(edge.from).is_some_and(|node| node.kind == *by)
                && self.node(edge.to).is_some_and(|node| node.kind == *of)
        })
    }

    /// The tasks with a `blocks` edge to `task`, in map order - what it
    /// waits on.
    pub fn blocked_by(&self, task: NodeId) -> Vec<&Node> {
        self.edges
            .iter()
            .filter(|edge| edge.kind == BLOCKS && edge.to == task)
            .filter_map(|edge| self.node(edge.from))
            .collect()
    }

    /// The options with an `answers` edge to `question`, in map order -
    /// the alternatives weighed against its decision. Older entries
    /// recorded the winning choice, and decisions it later superseded,
    /// as options too; those are dropped, so a caller is never handed an
    /// "alternative" that is really the decision under another kind.
    pub fn weighed_for(&self, question: NodeId) -> Vec<&Node> {
        let mut restated: HashSet<&str> = HashSet::new();
        for decision in self.settled_by(question) {
            restated.insert(decision.name.as_str());
            restated.extend(
                self.predecessors(decision.id)
                    .iter()
                    .map(|node| node.name.as_str()),
            );
        }
        self.edges
            .iter()
            .filter(|edge| edge.kind == ANSWERS && edge.to == question)
            .filter_map(|edge| self.node(edge.from))
            .filter(|node| node.kind == OPTION && !restated.contains(node.name.as_str()))
            .collect()
    }

    /// The questions with a `reopens` edge to `decision`, in map order.
    pub fn reopened_by(&self, decision: NodeId) -> Vec<&Node> {
        self.edges
            .iter()
            .filter(|edge| edge.kind == REOPENS && edge.to == decision)
            .filter_map(|edge| self.node(edge.from))
            .collect()
    }

    /// The decisions `question` reopens, in map order.
    pub fn reopens(&self, question: NodeId) -> Vec<&Node> {
        self.edges
            .iter()
            .filter(|edge| edge.kind == REOPENS && edge.from == question)
            .filter_map(|edge| self.node(edge.to))
            .collect()
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// When the map last gained a node or an edge; `None` while it is
    /// empty. A removal leaves no trace here - what was removed lives
    /// only in the events.
    #[cfg(feature = "lab")]
    pub fn last_changed(&self) -> Option<Timestamp> {
        let nodes = self.nodes.iter().map(|node| node.added_at);
        let edges = self.edges.iter().map(|edge| edge.added_at);
        nodes.chain(edges).max()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.by_id.get(&id).map(|&i| &self.nodes[i])
    }

    /// `id`'s standing - `None` when the map holds no such node, or when
    /// it is the human's own landmark (`Actor::Human`): a user-written
    /// node is never a claim to judge. Otherwise `Disputed` or
    /// `Confirmed` from the latest `claim.disputed`/`claim.confirmed`
    /// naming it, else `Seen` when a `review.finished` has, else
    /// `Claimed`.
    pub fn standing(&self, id: NodeId) -> Option<Standing> {
        let node = self.node(id)?;
        if matches!(node.actor, Actor::Human(_)) {
            return None;
        }
        Some(match self.judgments.get(&id).map(|j| &j.kind) {
            Some(JudgmentKind::Confirmed) => Standing::Confirmed,
            Some(JudgmentKind::Disputed(_)) => Standing::Disputed,
            None if self.reviewed.contains(&id) => Standing::Seen,
            None => Standing::Claimed,
        })
    }

    /// The why of the latest `claim.disputed` naming `id`, only while its
    /// standing is `Disputed` - `None` once a later `claim.confirmed`
    /// supersedes it.
    pub fn dispute(&self, id: NodeId) -> Option<&str> {
        if self.standing(id) != Some(Standing::Disputed) {
            return None;
        }
        match self.judgments.get(&id).map(|j| &j.kind) {
            Some(JudgmentKind::Disputed(why)) => Some(why.as_str()),
            _ => None,
        }
    }

    /// Every node whose latest judgment - `claim.confirmed` or
    /// `claim.disputed` - landed at or after `at`, each with the
    /// standing it now carries and when that judgment landed. A node
    /// only ever `review.finished` has named is not a judgment, so it
    /// is never in this list.
    pub fn judged_since(&self, at: Timestamp) -> impl Iterator<Item = (&Node, Standing, Timestamp)> {
        self.judgments.iter().filter_map(move |(id, judgment)| {
            if judgment.at < at {
                return None;
            }
            let node = self.node(*id)?;
            let standing = self.standing(*id)?;
            Some((node, standing, judgment.at))
        })
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
            .get(&(kind.name.clone(), seq))
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

    /// The map cut to what it gained since `at`: the nodes added then
    /// or later, plus the ends of every edge added then or later, so a
    /// new decision resolving an old question shows the question too.
    /// Edges from before `at` are not in the cut, even between kept
    /// nodes - they are not what changed.
    pub fn since(&self, at: Timestamp) -> Self {
        let fresh: Vec<&Edge> = self
            .edges
            .iter()
            .filter(|edge| edge.added_at >= at)
            .collect();
        let touched: HashSet<NodeId> = fresh.iter().flat_map(|edge| [edge.from, edge.to]).collect();
        let nodes = self
            .nodes
            .iter()
            .filter(|node| node.added_at >= at || touched.contains(&node.id))
            .cloned()
            .collect();
        let edges = fresh.into_iter().cloned().collect();
        let mut cut = Self::from_parts(self.schema.clone(), nodes, edges);
        cut.judgments = self.judgments.clone();
        cut.reviewed = self.reviewed.clone();
        cut
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
        let mut cut = Self::from_parts(self.schema.clone(), nodes, edges);
        cut.judgments = self.judgments.clone();
        cut.reviewed = self.reviewed.clone();
        cut
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
                if name.trim().is_empty() {
                    return Err(MapError::BlankName);
                }
                if self.find(kind, name).is_some() {
                    return Err(MapError::DuplicateNode {
                        kind: kind.clone(),
                        name: name.clone(),
                    });
                }
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
                    actor,
                    added_at: at,
                    seq,
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
                    actor,
                    added_at: at,
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
            // A node the fold no longer holds is ignored, not an error:
            // the log is append-only, and a node named here may since
            // have been removed.
            Payload::ClaimConfirmed { node, .. } => {
                if self.node(*node).is_some() {
                    self.judgments.insert(
                        *node,
                        Judgment {
                            at,
                            kind: JudgmentKind::Confirmed,
                        },
                    );
                }
            }
            Payload::ClaimDisputed { node, why, .. } => {
                if self.node(*node).is_some() {
                    self.judgments.insert(
                        *node,
                        Judgment {
                            at,
                            kind: JudgmentKind::Disputed(why.clone()),
                        },
                    );
                }
            }
            Payload::ReviewFinished { nodes, .. } => {
                for node in nodes {
                    if self.node(*node).is_some() {
                        self.reviewed.insert(*node);
                    }
                }
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
        | Payload::NodeRemoved { map, .. }
        | Payload::EdgeAdded { map, .. }
        | Payload::EdgeRemoved { map, .. }
        | Payload::ClaimConfirmed { map, .. }
        | Payload::ClaimDisputed { map, .. }
        | Payload::ReviewFinished { map, .. } => Some(map),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
