//! Why a write, or a stored event, does not fit its map. One error per
//! rule the map holds, each carrying the value that broke it and, where
//! an "expected one of" helps, what it could have been. `Display` is
//! the wording every surface shows: the CLI, the model's tool, and the
//! fold that rejects an event all read the same sentence.

use std::fmt;

use super::{EdgeEnd, NodeId, NodeRef};
use crate::core::{Actor, EventId, MapId};

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
    DuplicateMapIdentity {
        name: String,
        first: MapId,
        second: MapId,
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
    /// A change naming no rename, no property, and no source the node
    /// does not already cite: nothing would land, and it would still
    /// stamp the node. Write-only.
    EmptyChange,
    DuplicateNode {
        kind: String,
        name: String,
    },
    /// A property whose value is not among the values its kind declares
    /// for it. Write-only: checked by `Map::apply` on `AddNode` and
    /// `ChangeNode`, never by `replay`.
    UnknownValue {
        kind: String,
        property: String,
        value: String,
        values: Vec<String>,
    },
    /// A property outside the ones the kind declares. Write-only:
    /// checked by `Map::apply` on `AddNode` and `ChangeNode`, never by
    /// `replay`, so a node recorded before its kind's properties were
    /// declared still folds.
    UnknownProperty {
        kind: String,
        property: String,
        /// The kind's declared property names, in declared order - the
        /// list `apply` checks against.
        allowed: Vec<String>,
    },
    /// A rename, a property change, or a removal that W6's rank rule
    /// refuses: the actor neither owns nor outranks the writer, or is
    /// outranked by whoever touched it since. Write-only: a new edge is
    /// any actor's.
    NotYours {
        node: String,
        owner: Actor,
        touched_by: Actor,
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
            Self::DuplicateMapIdentity {
                name,
                first,
                second,
            } => write!(
                f,
                "map {name:?} has two identities: {} and {}",
                first.as_uuid(),
                second.as_uuid()
            ),
            Self::UnknownNodeKind { map, kinds, kind } => write!(
                f,
                "no node kind {kind:?} in map {map:?}; kinds are {kinds}"
            ),
            Self::UnknownEdgeKind { map, kinds, kind } => write!(
                f,
                "no edge kind {kind:?} in map {map:?}; kinds are {kinds}"
            ),
            Self::BlankName => write!(f, "a node's name must not be blank"),
            Self::EmptyChange => write!(
                f,
                "a change must name a rename, a property, or a source the node does not already cite"
            ),
            Self::DuplicateNode { kind, name } => {
                write!(f, "{kind} {name:?} is already in the map")
            }
            Self::UnknownProperty {
                kind,
                property,
                allowed,
            } => {
                if allowed.is_empty() {
                    write!(f, "{kind} has no property {property:?}; this kind carries none")
                } else {
                    write!(f, "{kind} has no property {property:?}; properties are {}", allowed.join(", "))
                }
            }
            Self::UnknownValue {
                kind,
                property,
                value,
                values,
            } => write!(
                f,
                "{kind} has no `{property}` {value:?}; {property} is {}",
                values.join(", ")
            ),
            Self::NotYours {
                node,
                owner,
                touched_by,
            } => write!(
                f,
                "{node} was written by {} and touched by {}; you may still add a node and an \
                 edge beside it",
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
