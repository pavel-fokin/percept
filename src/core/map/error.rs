//! Why a write, or a stored event, does not fit its map. One error per
//! rule the map holds, each carrying the value that broke it and, where
//! an "expected one of" helps, what it could have been. `Display` is
//! the wording every surface shows: the CLI, the model's tool, and the
//! fold that rejects an event all read the same sentence.

use std::fmt;

use super::{EdgeEnd, NodeId, NodeRef};
use crate::core::{Actor, EventId};

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
