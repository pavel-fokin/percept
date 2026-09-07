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
    pub name: &'static str,
    /// The one reasoning operation this map makes cheap, as a reader
    /// deciding whether to open it needs to hear it - what the prompt
    /// carries in place of the map.
    pub purpose: &'static str,
    pub node_kinds: &'static [Kind],
    pub edge_kinds: &'static [Kind],
    /// The node kinds worth a reader's attention without opening the
    /// whole map - what `MapShape::Headlines` sends.
    pub headline_kinds: &'static [&'static str],
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
    pub name: &'static str,
    pub gloss: &'static str,
}

/// The two node kinds a `resolves` edge joins: `by` settles `of`.
#[derive(Debug, PartialEq, Eq)]
pub struct Settlement {
    pub by: &'static str,
    pub of: &'static str,
}

/// The edge kind that corrects a decision: from the new one to the one
/// it replaces. The old node stays, one hop away, and leaves the
/// headlines - a removal would take a reader's landmark with it.
pub const SUPERSEDES: &str = "supersedes";

/// The edge kind from a decision to the question it settles.
pub const RESOLVES: &str = "resolves";

/// The edge kind from an option to the question it was weighed for.
pub const ANSWERS: &str = "answers";

/// The edge kind from a task to the task it must land before.
pub const BLOCKS: &str = "blocks";

/// What a decision settles.
const QUESTION: &str = "question";
/// The one node kind `revise_map` never removes: a decision is corrected
/// by a successor with a `supersedes` edge, so it is public.
pub const DECISION: &str = "decision";
/// An alternative that lost. The store refuses one that does not say
/// why, so it is public too.
pub const OPTION: &str = "option";
/// One piece of work left to do. The store refuses one that does not
/// say why it matters, so it is public.
pub const TASK: &str = "task";
/// What became of a task: done, with the commit it landed in, or
/// dropped, with the reason.
pub const OUTCOME: &str = "outcome";

/// The decision map: what was asked, what was weighed, what was chosen
/// and on what grounds. An option `answers` its question, evidence
/// `supports` or `contradicts` an option, a decision `resolves` the
/// question, and a later decision `supersedes` an earlier one - so
/// `--around` a question reaches everything weighed for it.
pub const DECISIONS: Schema = Schema {
    name: "decisions",
    purpose: "what was asked, what was chosen, and why, so a settled question is not reopened",
    node_kinds: &[
        Kind {
            name: QUESTION,
            gloss: "a matter the project had to settle",
        },
        Kind {
            name: OPTION,
            gloss: "an alternative that was weighed and lost, saying why in its `why` property",
        },
        Kind {
            name: "evidence",
            gloss: "a fact that supports or contradicts an option",
        },
        Kind {
            name: DECISION,
            gloss: "the choice that was made, and the grounds for it",
        },
    ],
    edge_kinds: &[
        Kind {
            name: ANSWERS,
            gloss: "from an option to the question it was weighed for",
        },
        Kind {
            name: "supports",
            gloss: "from evidence to an option it backs",
        },
        Kind {
            name: "contradicts",
            gloss: "from evidence to an option it undercuts",
        },
        Kind {
            name: RESOLVES,
            gloss: "from a decision to the question it settles",
        },
        Kind {
            name: SUPERSEDES,
            gloss: "from a decision to an earlier one it replaces",
        },
    ],
    headline_kinds: &[QUESTION, DECISION],
    settlement: Some(Settlement {
        by: DECISION,
        of: QUESTION,
    }),
};

/// The tasks map: what is left to do. A task says why it matters, an
/// outcome `resolves` it - done, or dropped and why - a task `blocks`
/// the one that must wait for it, and a rewritten task `supersedes`
/// the old wording, never removes it.
pub const TASKS: Schema = Schema {
    name: "tasks",
    purpose: "what is left to do, why it matters, and what it waits on, so a session picks up the next item without re-deriving it",
    node_kinds: &[
        Kind { name: TASK, gloss: "one piece of work left to do, saying why it matters in its `why` property" },
        Kind { name: OUTCOME, gloss: "what became of a task: done with its commit, or dropped with the reason" },
    ],
    edge_kinds: &[
        Kind { name: RESOLVES, gloss: "from an outcome to the task it settles" },
        Kind { name: BLOCKS, gloss: "from a task to the one that must wait for it" },
        Kind { name: SUPERSEDES, gloss: "from a reworded task to the wording it replaces" },
    ],
    headline_kinds: &[TASK],
    settlement: Some(Settlement {
        by: OUTCOME,
        of: TASK,
    }),
};

/// The code map: a codebase's files, the symbols they define, and what
/// imports what. Derived from the working tree, never folded from the
/// log, so it is in `DERIVED` and not `SCHEMAS`.
pub const CODE: Schema = Schema {
    name: "code",
    purpose: "which file defines which symbol and imports which file or package",
    node_kinds: &[
        Kind { name: "file", gloss: "a source file, named by its repo-relative path" },
        Kind { name: "function", gloss: "a function or method, named `path::Type::method` or `path::func`" },
        Kind { name: "type", gloss: "a struct, enum, trait, or alias, named `path::Name`" },
        Kind {
            name: "package",
            gloss: "an external crate a file imports, like `serde_json` - never one of this project's own modules",
        },
    ],
    edge_kinds: &[
        Kind { name: "contains", gloss: "from a file to a symbol it defines" },
        Kind { name: "imports", gloss: "from a file to a file or package it uses" },
    ],
    headline_kinds: &["file"],
    settlement: None,
};

/// Every map folded from the log. One map per schema, named after it.
pub const SCHEMAS: &[&Schema] = &[&DECISIONS, &TASKS];

/// Every map derived from something other than the log. A reader
/// builds one fresh; no writer commits to it.
pub const DERIVED: &[&Schema] = &[&CODE];

/// Whether `name` names a map in `DERIVED`.
fn is_derived(name: &str) -> bool {
    DERIVED.iter().any(|schema| schema.name == name)
}

impl Schema {
    /// Whether this map is built from something other than the log, so
    /// its nodes have no history: no writer, no moment they were added.
    pub fn is_derived(&self) -> bool {
        is_derived(self.name)
    }

    /// The node kind names, in schema order.
    pub fn node_kind_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.node_kinds.iter().map(|kind| kind.name)
    }

    /// The edge kind names, in schema order.
    pub fn edge_kind_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.edge_kinds.iter().map(|kind| kind.name)
    }

    /// The log-folded schema `name` names, or the error every boundary
    /// that folds or writes a map by name reports. A derived map is its
    /// own error: it exists, and this is the wrong door to it.
    pub fn find(name: &str) -> Result<&'static Schema, MapError> {
        if is_derived(name) {
            return Err(MapError::Derived(name.to_string()));
        }
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
    /// Who added this node - the actor its `node.added` event carried.
    pub actor: Actor,
    /// When this node was added - that event's `created_at`.
    pub added_at: Timestamp,
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
    UnknownMap(String),
    /// A map in `DERIVED`, named where only a log-folded map fits.
    Derived(String),
    /// `since` on a `DERIVED` map: it is walked fresh, so it has no
    /// "before". One rule for the CLI and the `read_map` tool.
    SinceOnDerived(&'static str),
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
    NoSuchNode {
        node: NodeRef,
        /// Nodes of the same kind whose name overlaps `node.name`, as
        /// `kind:name` a reader can paste back. Empty when none do.
        suggestions: Vec<String>,
    },
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
                    .chain(DERIVED)
                    .map(|schema| schema.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Derived(name) => write!(
                f,
                "{name:?} is derived from the working tree, not the log: read it \
                 with read_map or `percept maps show {name}`"
            ),
            Self::SinceOnDerived(name) => write!(
                f,
                "since has no meaning for {name}: it is walked fresh from the \
                 working tree and has no history"
            ),
            Self::UnknownNodeKind { map, kind } => write!(
                f,
                "no node kind {kind:?} in map {:?}; kinds are {}",
                map.name,
                map.node_kind_names().collect::<Vec<_>>().join(", ")
            ),
            Self::UnknownEdgeKind { map, kind } => write!(
                f,
                "no edge kind {kind:?} in map {:?}; kinds are {}",
                map.name,
                map.edge_kind_names().collect::<Vec<_>>().join(", ")
            ),
            Self::BlankName => write!(f, "a node's name must not be blank"),
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
    // Indexes over `nodes` and `edges`, kept in step by `replay`, so a
    // lookup by id, by name, or by edge is a hash rather than a scan:
    // every `apply` looks up its ends, and a code map applies tens of
    // thousands of them.
    by_id: HashMap<NodeId, usize>,
    by_name: HashMap<(String, String), NodeId>,
    edge_keys: HashSet<(String, NodeId, NodeId)>,
}

impl Map {
    pub fn empty(schema: &'static Schema) -> Self {
        Self::from_parts(schema, Vec::new(), Vec::new())
    }

    fn from_parts(schema: &'static Schema, nodes: Vec<Node>, edges: Vec<Edge>) -> Self {
        let by_id = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let by_name = nodes
            .iter()
            .map(|n| ((n.kind.clone(), n.name.clone()), n.id))
            .collect();
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
            edge_keys,
        }
    }

    /// Folds the events that belong to `schema`'s map and fall inside
    /// `scope`, in the order given, which must be log order. Events for
    /// other maps, of other kinds, or outside `scope` are skipped. An
    /// event that breaks a rule is an error naming it, not skipped:
    /// silently dropping it would hide that something went wrong at
    /// write time.
    pub fn fold<'a>(
        schema: &'static Schema,
        scope: &Scope,
        events: impl IntoIterator<Item = &'a Event>,
    ) -> Result<Self, MapError> {
        let mut map = Self::empty(schema);
        for event in events {
            if !scope.admits(event) {
                continue;
            }
            if map_of(event.payload()) != Some(schema.name) {
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

    /// Every map percept knows, folded from `events` within `scope`.
    pub fn fold_all<'a>(
        scope: &Scope,
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Self>, MapError> {
        SCHEMAS
            .iter()
            .map(|schema| Self::fold(schema, scope, events.clone()))
            .collect()
    }

    pub fn schema(&self) -> &'static Schema {
        self.schema
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The nodes of the schema's headline kinds, in map order, less the
    /// superseded ones - what a reader sees of the map before opening
    /// it.
    pub fn headlines(&self) -> impl Iterator<Item = &Node> {
        let kinds = self.schema.headline_kinds;
        self.nodes
            .iter()
            .filter(move |node| kinds.contains(&node.kind.as_str()))
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

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// When the map last gained a node or an edge; `None` while it is
    /// empty. A removal leaves no trace here - what was removed lives
    /// only in the events.
    pub fn last_changed(&self) -> Option<Timestamp> {
        let nodes = self.nodes.iter().map(|node| node.added_at);
        let edges = self.edges.iter().map(|edge| edge.added_at);
        nodes.chain(edges).max()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.by_id.get(&id).map(|&i| &self.nodes[i])
    }

    pub fn find(&self, kind: &str, name: &str) -> Option<&Node> {
        let id = self.by_name.get(&(kind.to_string(), name.to_string()))?;
        self.node(*id)
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
        Self::from_parts(self.schema, nodes, edges)
    }

    /// The map cut to `selection`, in its fixed order, counting what
    /// the cut left out. Consumes the map: a whole selection is the map
    /// itself, not a copy.
    pub fn select(self, selection: &Selection) -> Result<Fragment, MapError> {
        if selection.since.is_some() && self.schema.is_derived() {
            return Err(MapError::SinceOnDerived(self.schema.name));
        }
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
        Self::from_parts(self.schema, nodes, edges)
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
                self.by_id.insert(*node, self.nodes.len());
                self.by_name.insert((kind.clone(), name.clone()), *node);
                self.nodes.push(Node {
                    id: *node,
                    kind: kind.clone(),
                    name: name.clone(),
                    properties: properties.clone(),
                    sources: sources.clone(),
                    actor,
                    added_at: at,
                });
            }
            Payload::NodeRemoved { node, .. } => {
                let removed = self.node(*node).ok_or(MapError::NoSuchNodeId(*node))?;
                let key = (removed.kind.clone(), removed.name.clone());
                self.by_name.remove(&key);
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
                map: self.schema,
                kind: kind.to_string(),
            })
        }
    }

    fn check_edge_kind(&self, kind: &str) -> Result<(), MapError> {
        if self.schema.edge_kind_names().any(|name| name == kind) {
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
        | Payload::EdgeRemoved { map, .. } => Some(map),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
