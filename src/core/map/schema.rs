//! A map's schema: the node and edge kinds it allows, the properties
//! and states each kind demands, and the set of schemas a project has.
//! Data, not an enum - a project adds a map by adding a TOML file, and
//! `mapstore` loads these values from it. `Map` checks every write
//! against the schema it was folded with.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{Map, MapError};
use crate::core::Event;

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
    /// The lines this schema injects into an agent's context, by
    /// moment.
    pub rules: Rules,
}

/// The lines a schema injects into an agent's context, by moment - a
/// map from moment name to lines, so a fourth moment costs no new
/// field. Which moments have a channel to speak on is `mapstore`'s to
/// know, not this type's; `Rules` only holds what a schema declared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules(BTreeMap<String, Vec<String>>);

impl Rules {
    /// `moments`, each moment's lines as declared - the constructor
    /// every loader builds from.
    pub fn new(moments: BTreeMap<String, Vec<String>>) -> Self {
        Self(moments)
    }

    /// The lines declared for `moment`, empty when the schema declared
    /// none for it.
    pub fn at(&self, moment: &str) -> &[String] {
        self.0.get(moment).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Every moment this schema declares at least one line for, moment
    /// name paired with its lines, in moment-name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.0
            .iter()
            .map(|(moment, lines)| (moment.as_str(), lines.as_slice()))
            .filter(|(_, lines)| !lines.is_empty())
    }

    /// Whether every moment's list is empty - no lines to send at any
    /// moment, so a caller carries no attribution for none sent.
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
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
    /// The properties a node of this kind may carry beyond `requires` -
    /// `note` on a `fact`. A write refuses any property outside
    /// `requires`, `properties`, and `state` when `states` is
    /// non-empty; checked on a write, never on a fold, so what was
    /// recorded before the rule still folds.
    pub properties: Vec<String>,
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
            properties: Vec::new(),
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

    /// `self`, with `properties` as the optional properties beyond
    /// `requires` a node of this kind may carry. Used only by
    /// `core::testing`'s fixture schemas today, so `cfg(test)` too.
    #[cfg(test)]
    pub(crate) fn with_properties(mut self, properties: &[&str]) -> Self {
        self.properties = properties.iter().map(|p| p.to_string()).collect();
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
    /// requires and may carry - `` `option` (requires `why`, may carry
    /// `note`) `` - for a csv or a rendered list, so the one shape is
    /// built once and read everywhere a kind is named.
    pub fn label(&self) -> String {
        let ticked = |names: &[String]| {
            let ticked: Vec<String> = names.iter().map(|p| format!("`{p}`")).collect();
            ticked.join(", ")
        };
        let mut parts: Vec<String> = Vec::new();
        if !self.requires.is_empty() {
            parts.push(format!("requires {}", ticked(&self.requires)));
        }
        if !self.properties.is_empty() {
            parts.push(format!("may carry {}", ticked(&self.properties)));
        }
        if parts.is_empty() {
            format!("`{}`", self.kind)
        } else {
            format!("`{}` ({})", self.kind, parts.join(", "))
        }
    }

    /// Every property a write may put on a node of this kind, in the
    /// order an error lists them: `requires`, `properties`, then
    /// `state` when the kind declares states. The one place that rule
    /// lives; `allows` and `Map::apply` read through it.
    pub fn allowed_properties(&self) -> impl Iterator<Item = &str> {
        self.requires
            .iter()
            .chain(&self.properties)
            .map(String::as_str)
            .chain((!self.states.is_empty()).then_some("state"))
    }

    pub fn allows(&self, property: &str) -> bool {
        self.allowed_properties().any(|allowed| allowed == property)
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

    /// Every created map `Map::fold` gives for `events`, in schema
    /// order. A schema with no `map.created` event is not a map yet.
    pub fn fold_all<'a>(
        &self,
        events: impl IntoIterator<Item = &'a Event> + Clone,
    ) -> Result<Vec<Map>, MapError> {
        let mut maps = Vec::new();
        for schema in self.folded() {
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
