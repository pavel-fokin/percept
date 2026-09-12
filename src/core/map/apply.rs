//! The write path: `Map::apply` checks a `Mutation` against the schema
//! and the map's current state, then returns the `Payload` that records
//! it, and `replay` folds that payload back in. Every writer - the CLI,
//! the model's tool, the review page - goes through `apply`, so the six
//! write rules and the rank lock live here and nowhere else.

use std::collections::{BTreeMap, HashSet};

use super::{
    is_path_like, segments, Actor, Change, Edge, EdgeEnd, Map, MapError, Mutation, Node, NodeId,
    NodeKind, NodeRef, Payload, Written,
};
use crate::shared::Timestamp;

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
pub(super) fn highest(actors: impl Iterator<Item = Actor>) -> Option<Actor> {
    actors.max_by_key(|actor| rank(*actor))
}

/// Whether `why`, when given, is blank - W2's `BlankWhy`.
fn blank_why(why: Option<&str>) -> Result<(), MapError> {
    match why {
        Some(why) if why.trim().is_empty() => Err(MapError::BlankWhy),
        _ => Ok(()),
    }
}

impl Map {
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
    pub(super) fn replay(&mut self, payload: &Payload, actor: Actor, at: Timestamp) -> Result<(), MapError> {
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

    pub(super) fn resolve(&self, node: NodeRef) -> Result<NodeId, MapError> {
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

    pub(super) fn check_node_kind(&self, kind: &str) -> Result<(), MapError> {
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
