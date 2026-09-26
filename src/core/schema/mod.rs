//! A map's schema: the node and edge kinds it allows, the properties
//! each node kind carries, and the set of schemas a project has.
//! Data, not an enum - a project adds a map by adding a TOML file, and
//! `mapstore` loads these values from it. `Map` checks every write
//! against the schema it was folded with.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use super::{Event, Map, MapError, MapId, Source};

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

/// The schemas a project has: every one, folded from the log. A port -
/// `mapstore::SchemaCatalog` is the concrete loader, reading a
/// project's own TOML files; every fold, write, and error message
/// about an unknown map goes through this, so no caller keeps its own
/// list.
pub trait Schemas: Send + Sync {
    /// Every schema, in stored order - a global schema, one whose map
    /// lives at `$HOME` rather than a project, sorts before every
    /// project schema.
    fn folded(&self) -> &[Arc<Schema>];

    /// `Some(home)` when `name` names a global schema - one loaded from
    /// `$HOME/.percept/schemas`, whose map's identity lives at `home`
    /// rather than at whatever project reads it. `None` for a project
    /// schema, whose map lives at the project being read.
    fn global_root(&self, name: &str) -> Option<&Path>;

    /// The schema `name` names, or the error every boundary that folds
    /// or writes a map by name reports.
    fn find(&self, name: &str) -> Result<Arc<Schema>, MapError> {
        self.folded()
            .iter()
            .find(|schema| schema.name == name)
            .cloned()
            .ok_or_else(|| MapError::UnknownMap {
                name: name.to_string(),
                maps: names_csv(self.folded()),
            })
    }
}

/// The rules a project's schemas keep together, so a word a reader
/// types names one thing across every map: no schema's name takes a
/// short id's form, and no node kind, nor its short id prefix, is
/// declared by two schemas. Within one schema `Schema::new` already
/// holds the prefix rule. Edge kinds may repeat - an edge finds its
/// map from its nodes.
pub fn check_across(schemas: &[Arc<Schema>]) -> Result<(), SchemaError> {
    if let Some(schema) = schemas.iter().find(|schema| super::map::split_short_id(&schema.name).is_some()) {
        return Err(SchemaError::NameIsShortId {
            name: schema.name.clone(),
        });
    }
    let kinds: Vec<(&str, &NodeKind)> = schemas
        .iter()
        .flat_map(|schema| schema.node_kinds.iter().map(move |kind| (schema.name.as_str(), kind)))
        .collect();
    for (at, (second, kind)) in kinds.iter().enumerate() {
        let earlier = &kinds[..at];
        if let Some((first, _)) = earlier.iter().find(|(_, other)| other.kind == kind.kind) {
            return Err(SchemaError::KindInTwoSchemas {
                kind: kind.kind.clone(),
                first: first.to_string(),
                second: second.to_string(),
            });
        }
        if let Some((first, _)) = earlier.iter().find(|(_, other)| other.prefix == kind.prefix) {
            return Err(SchemaError::PrefixInTwoSchemas {
                prefix: kind.prefix.clone(),
                first: first.to_string(),
                second: second.to_string(),
                kind: kind.kind.clone(),
                fix: free_prefix(&kind.kind, &kinds),
            });
        }
    }
    Ok(())
}

/// The shortest longer start of `name`, lowercased, that no kind in
/// `kinds` takes as its prefix - `cl` for `claim` beside `concept`.
fn free_prefix(name: &str, kinds: &[(&str, &NodeKind)]) -> Option<String> {
    let lower = name.to_lowercase();
    let starts = lower.char_indices().skip(1).map(|(at, _)| &lower[..at]);
    let free = starts
        .chain([lower.as_str()])
        .find(|prefix| kinds.iter().all(|(_, kind)| kind.prefix != *prefix));
    free.map(str::to_string)
}

/// The schema declaring node kind `kind` - unique across every loaded
/// schema, since `check_across` refuses two that would. `None` when no
/// schema declares it. What `add`/`remove` resolve a node kind to its
/// one map through, with no map name in the command.
pub fn schema_of_node_kind(schemas: &dyn Schemas, kind: &str) -> Option<Arc<Schema>> {
    schemas
        .folded()
        .iter()
        .find(|schema| schema.node_kind(kind).is_some())
        .cloned()
}

/// The schema a node ref names: `kind:name` by its kind, or the short
/// id `s` takes by the node kind prefix it starts with. `None` when
/// neither resolves - an unknown kind, or a prefix no schema declares.
/// What `add covers c1 c3` resolves each end's map through, before
/// checking the two agree.
pub fn schema_of_ref(schemas: &dyn Schemas, s: &str) -> Option<Arc<Schema>> {
    match s.split_once(':') {
        Some((kind, _)) => schema_of_node_kind(schemas, kind),
        None => {
            let (prefix, _) = super::map::split_short_id(s)?;
            schemas
                .folded()
                .iter()
                .find(|schema| schema.node_kinds().iter().any(|kind| kind.prefix() == prefix))
                .cloned()
        }
    }
}

/// Whether any schema declares `kind` as an edge kind - edge kinds may
/// repeat across schemas, so this only says the word is known, not
/// which map it belongs to.
pub fn edge_kind_declared(schemas: &dyn Schemas, kind: &str) -> bool {
    schemas.folded().iter().any(|schema| schema.edge_kind(kind).is_some())
}

/// The root a map named `name` lives at: `schemas.global_root(name)`
/// when `name` names a global schema, else `project`. Every identity
/// lookup and every `map.created` goes through this, so "a schema's
/// map root" means one thing everywhere it is found or minted.
pub fn map_root<'a>(schemas: &'a dyn Schemas, name: &str, project: &'a Path) -> &'a Path {
    schemas.global_root(name).unwrap_or(project)
}

/// The map `name` names, if it has been created: its `map.created`
/// among those of `events` written at the map's root.
pub fn map_id_at<'a>(
    schemas: &dyn Schemas,
    name: &str,
    events: impl IntoIterator<Item = &'a Event>,
    project: &Path,
) -> Result<Option<MapId>, MapError> {
    let root = map_root(schemas, name, project);
    super::map_id_for(name, events.into_iter().filter(|event| event.source().path == root))
}

/// A `map.created` for the map `name` names, written at its root under
/// `source`'s name - how a map comes into being wherever it is minted.
pub fn map_created_at(schemas: &dyn Schemas, id: MapId, name: &str, source: &Source) -> Event {
    let root = Source {
        name: source.name.clone(),
        path: map_root(schemas, name, &source.path).to_path_buf(),
    };
    Event::map_created(id, name.to_string(), root)
}

/// Every created map `Map::fold` gives for `events`, in schema order.
/// A schema with no `map.created` event is not a map yet.
pub fn fold_all<'a>(
    schemas: &dyn Schemas,
    events: impl IntoIterator<Item = &'a Event> + Clone,
) -> Result<Vec<Map>, MapError> {
    fold_matching(schemas, |_| true, events)
}

/// `fold_all`, restricted to the schemas named in `names` - what a
/// batch of commits actually touches, so a caller checking that a
/// batch fits its own map never refolds every other schema too.
pub fn fold_named<'a>(
    schemas: &dyn Schemas,
    names: &HashSet<String>,
    events: impl IntoIterator<Item = &'a Event> + Clone,
) -> Result<Vec<Map>, MapError> {
    fold_matching(schemas, |schema| names.contains(&schema.name), events)
}

fn fold_matching<'a>(
    schemas: &dyn Schemas,
    matches: impl Fn(&Schema) -> bool,
    events: impl IntoIterator<Item = &'a Event> + Clone,
) -> Result<Vec<Map>, MapError> {
    let mut maps = Vec::new();
    for schema in schemas.folded().iter().filter(|schema| matches(schema)) {
        let Some(id) = super::map_id_for(&schema.name, events.clone())? else {
            continue;
        };
        maps.push(Map::fold(id, schema.clone(), events.clone())?);
    }
    Ok(maps)
}

/// Every schema's name, in stored order, for an "expected one of"
/// error.
fn names_csv(schemas: &[Arc<Schema>]) -> String {
    csv_or_none(schemas.iter().map(|schema| schema.name.clone()))
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
