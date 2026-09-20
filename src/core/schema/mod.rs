//! A map's schema: the node and edge kinds it allows, the properties
//! each node kind carries, and the set of schemas a project has.
//! Data, not an enum - a project adds a map by adding a TOML file, and
//! `mapstore` loads these values from it. `Map` checks every write
//! against the schema it was folded with.

use std::collections::HashSet;
use std::sync::Arc;

use super::{Event, Map, MapError};

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
    pub kind: String,
    /// This node kind's short id prefix - `d` for `decision`, so a
    /// node reads as `d41` rather than its full id.
    pub prefix: String,
    /// Property name paired with the values it may hold, in the order
    /// declared - the order an error lists them and a render prints
    /// them.
    pub properties: Vec<(String, Vec<String>)>,
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
    pub(crate) fn new(kind: &str) -> Self {
        Self {
            prefix: default_prefix(kind),
            kind: kind.to_string(),
            properties: Vec::new(),
        }
    }

    /// `self`, with `properties` declared in order - each name paired
    /// with the values it may hold, empty for free text. Used only by
    /// `core::testing`'s fixture schemas and this module's own tests,
    /// so `cfg(test)`.
    #[cfg(test)]
    pub(crate) fn with_properties(mut self, properties: &[(&str, &[&str])]) -> Self {
        self.properties = properties
            .iter()
            .map(|(name, values)| {
                (name.to_string(), values.iter().map(|v| v.to_string()).collect())
            })
            .collect();
        self
    }

    /// This kind's name, backticked, alone or with the properties it
    /// carries - `` `option` (carries `why`, `note`) `` - for a csv or
    /// a rendered list, so the one shape is built once and read
    /// everywhere a kind is named.
    pub fn label(&self) -> String {
        if self.properties.is_empty() {
            format!("`{}`", self.kind)
        } else {
            let names: Vec<String> =
                self.properties.iter().map(|(name, _)| format!("`{name}`")).collect();
            format!("`{}` (carries {})", self.kind, names.join(", "))
        }
    }

    /// The property named `name`, when this kind declares it.
    pub fn property(&self, name: &str) -> Option<&(String, Vec<String>)> {
        self.properties.iter().find(|(declared, _)| declared == name)
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
    pub kind: String,
    pub from: Vec<String>,
    pub to: Vec<String>,
}

impl EdgeKind {
    pub(crate) fn new(kind: &str, from: &[&str], to: &[&str]) -> Self {
        Self {
            kind: kind.to_string(),
            from: from.iter().map(|s| s.to_string()).collect(),
            to: to.iter().map(|s| s.to_string()).collect(),
        }
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
