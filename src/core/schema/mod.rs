//! A map's schema: the node and edge kinds it allows, the properties
//! each node kind carries, and the set of schemas a project has.
//! Data, not an enum - a project adds a map by adding a TOML file, and
//! `mapstore` loads these values from it. `Map` checks every write
//! against the schema it was folded with.

use std::collections::HashSet;
use std::sync::Arc;

use super::{Event, Map, MapError};

mod error;

pub use error::SchemaError;

/// Which node and edge kinds a map allows. Data, not an enum: adding a
/// map is adding a value.
#[derive(Debug, PartialEq, Eq)]
pub struct Schema {
    name: String,
    /// The one reasoning operation this map makes cheap, as a reader
    /// deciding whether to open it needs to hear it - what the prompt
    /// carries in place of the map.
    purpose: String,
    node_kinds: Vec<NodeKind>,
    edge_kinds: Vec<EdgeKind>,
}

/// A node kind: its short id prefix and the properties a node of this
/// kind may carry, in declared order. Each property names the values it
/// may hold - empty for free text, like `why` on a `fact`; non-empty
/// for a closed list, like `state` on a `task`. The first property with
/// a closed list is this kind's own: a node that carries none starts
/// from its first value, so a reader always finds one, even on a node
/// that never wrote it.
#[derive(Debug, PartialEq, Eq)]
pub struct NodeKind {
    kind: String,
    /// This node kind's short id prefix - `d` for `decision`, so a
    /// node reads as `d41` rather than its full id.
    prefix: String,
    /// Property name paired with the values it may hold, in the order
    /// declared - the order an error lists them and a render prints
    /// them.
    properties: Vec<(String, Vec<String>)>,
}

/// A kind's prefix when its schema names none: the name's own first
/// character, lowercased - `d` for `decision`, `t` for `task`. Used
/// by `NodeKind::new`, so the rule for "no prefix given" lives in the
/// domain rather than its file format.
fn default_prefix(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_lowercase().to_string())
        .unwrap_or_default()
}

fn repeated<'a>(names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = Vec::new();
    for name in names {
        if seen.contains(&name) {
            return Some(name);
        }
        seen.push(name);
    }
    None
}

impl NodeKind {
    pub fn new(
        kind: impl Into<String>,
        properties: Vec<(String, Vec<String>)>,
    ) -> Result<Self, SchemaError> {
        let kind = kind.into();
        let prefix = default_prefix(&kind);
        Self::with_prefix(kind, prefix, properties)
    }

    pub fn with_prefix(
        kind: impl Into<String>,
        prefix: impl Into<String>,
        properties: Vec<(String, Vec<String>)>,
    ) -> Result<Self, SchemaError> {
        let kind = kind.into();
        let prefix = prefix.into();
        if kind.trim().is_empty() {
            return Err(SchemaError::BlankKind { group: "node" });
        }
        let mut closed = None;
        for (name, values) in &properties {
            if name.trim().is_empty() {
                return Err(SchemaError::BlankProperty { kind });
            }
            if values.is_empty() {
                continue;
            }
            if let Some(first) = closed {
                return Err(SchemaError::SecondClosedList {
                    kind,
                    property: name.clone(),
                    first,
                });
            }
            if values.len() < 2 {
                return Err(SchemaError::TooFewValues {
                    kind,
                    property: name.clone(),
                });
            }
            if values.iter().any(|value| value.trim().is_empty()) {
                return Err(SchemaError::BlankValue {
                    kind,
                    property: name.clone(),
                });
            }
            if let Some(value) = repeated(values.iter().map(String::as_str)) {
                return Err(SchemaError::RepeatedValue {
                    kind,
                    property: name.clone(),
                    value: value.to_string(),
                });
            }
            closed = Some(name.clone());
        }
        Ok(Self {
            kind,
            prefix,
            properties,
        })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn properties(&self) -> &[(String, Vec<String>)] {
        &self.properties
    }

    /// This kind's name, backticked, alone or with the properties it
    /// carries - `` `option` (carries `why`, `note`) `` - for a csv or
    /// a rendered list, so the one shape is built once and read
    /// everywhere a kind is named.
    pub fn label(&self) -> String {
        if self.properties.is_empty() {
            format!("`{}`", self.kind)
        } else {
            let names: Vec<String> = self
                .properties
                .iter()
                .map(|(name, _)| format!("`{name}`"))
                .collect();
            format!("`{}` (carries {})", self.kind, names.join(", "))
        }
    }

    /// The property named `name`, when this kind declares it.
    pub fn property(&self, name: &str) -> Option<&(String, Vec<String>)> {
        self.properties
            .iter()
            .find(|(declared, _)| declared == name)
    }

    /// This kind's closed list - the first property with a non-empty
    /// list of values, name and values both - or `None` when every
    /// property this kind declares is free text.
    pub fn closed_list(&self) -> Option<(&str, &[String])> {
        self.properties
            .iter()
            .find(|(_, values)| !values.is_empty())
            .map(|(name, values)| (name.as_str(), values.as_slice()))
    }

    /// Every property a write may put on a node of this kind, in
    /// declared order - what `Map::apply` lists in an "expected one of"
    /// error.
    pub fn allowed_properties(&self) -> impl Iterator<Item = &str> {
        self.properties.iter().map(|(name, _)| name.as_str())
    }
}

/// An edge kind and the node kinds it may join: `from` on the tail,
/// `to` on the head, each naming one or more node kinds by name.
/// `Map::apply` refuses an `AddEdge` whose ends are not of these kinds.
#[derive(Debug, PartialEq, Eq)]
pub struct EdgeKind {
    kind: String,
    from: Vec<String>,
    to: Vec<String>,
}

impl EdgeKind {
    pub fn new(
        kind: impl Into<String>,
        from: Vec<String>,
        to: Vec<String>,
    ) -> Result<Self, SchemaError> {
        let kind = kind.into();
        if kind.trim().is_empty() {
            return Err(SchemaError::BlankKind { group: "edge" });
        }
        if from.is_empty() {
            return Err(SchemaError::EmptyEdgeEnd {
                edge: kind,
                end: "from",
            });
        }
        if to.is_empty() {
            return Err(SchemaError::EmptyEdgeEnd {
                edge: kind,
                end: "to",
            });
        }
        Ok(Self { kind, from, to })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn from(&self) -> &[String] {
        &self.from
    }

    pub fn to(&self) -> &[String] {
        &self.to
    }

    /// This kind's name, backticked, with its ends - `` `contains`
    /// (file -> function | type) `` - so a reader meets the direction
    /// alongside the name.
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

    /// Every created map `Map::fold` gives for `events`, in schema
    /// order. A schema with no `map.created` event is not a map yet.
    pub fn fold_all<'a>(
        &self,
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Map>, MapError> {
        self.fold_matching(|_| true, events)
    }

    /// `fold_all`, restricted to the schemas named in `names` - what a
    /// batch of commits actually touches, so a caller checking that a
    /// batch fits its own map never refolds every other schema too.
    pub fn fold_named<'a>(
        &self,
        names: &HashSet<String>,
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Map>, MapError> {
        self.fold_matching(|schema| names.contains(&schema.name), events)
    }

    fn fold_matching<'a>(
        &self,
        matches: impl Fn(&Schema) -> bool,
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Map>, MapError> {
        let mut maps = Vec::new();
        for schema in self.folded().filter(|schema| matches(schema)) {
            let Some(id) = super::map_id_for(&schema.name, events.clone())? else {
                continue;
            };
            maps.push(Map::fold(id, schema.clone(), events.clone())?);
        }
        Ok(maps)
    }

    /// Every schema's name, in stored order, for an "expected one of"
    /// error.
    fn names_csv(&self) -> String {
        csv_or_none(self.schemas.iter().map(|schema| schema.name.clone()))
    }
}

/// `items` joined by `, ` for an "expected one of" message - `none`
/// when there is nothing to expect, so the message never ends on a
/// dangling "are ".
fn csv_or_none(items: impl Iterator<Item = String>) -> String {
    let csv = items.collect::<Vec<_>>().join(", ");
    if csv.is_empty() {
        "none".to_string()
    } else {
        csv
    }
}

impl Schema {
    pub fn new(
        name: impl Into<String>,
        purpose: impl Into<String>,
        node_kinds: Vec<NodeKind>,
        edge_kinds: Vec<EdgeKind>,
    ) -> Result<Self, SchemaError> {
        if node_kinds.is_empty() {
            return Err(SchemaError::NoNodeKinds);
        }
        let mut seen: Vec<&NodeKind> = Vec::new();
        for kind in &node_kinds {
            if let Some(other) = seen.iter().find(|other| other.prefix == kind.prefix) {
                return Err(SchemaError::PrefixCollision {
                    first: other.kind.clone(),
                    second: kind.kind.clone(),
                    prefix: kind.prefix.clone(),
                });
            }
            seen.push(kind);
        }
        for edge in &edge_kinds {
            for (end, kinds) in [("from", &edge.from), ("to", &edge.to)] {
                for kind in kinds {
                    if !node_kinds.iter().any(|node| node.kind == *kind) {
                        return Err(SchemaError::UnknownEdgeEnd {
                            edge: edge.kind.clone(),
                            end,
                            kind: kind.clone(),
                        });
                    }
                }
            }
        }
        Ok(Self {
            name: name.into(),
            purpose: purpose.into(),
            node_kinds,
            edge_kinds,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn node_kinds(&self) -> &[NodeKind] {
        &self.node_kinds
    }

    pub fn edge_kinds(&self) -> &[EdgeKind] {
        &self.edge_kinds
    }

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
        csv_or_none(self.node_kinds.iter().map(NodeKind::label))
    }

    /// The edge kinds' labels as a `, `-joined list.
    pub fn edge_kinds_csv(&self) -> String {
        csv_or_none(self.edge_kinds.iter().map(EdgeKind::label))
    }
}

#[cfg(test)]
mod tests;
