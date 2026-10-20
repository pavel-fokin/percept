use std::collections::BTreeMap;
use std::path::PathBuf;

use jiff::Timestamp;

use super::{Actor, Event, EventId, Payload};
use crate::shared::{Id, Result};

pub type MapId = Id<Map>;
pub type NodeId = Id<Node>;

/// A named body of understanding in one project, created by a `MapCreated` event.
/// Its nodes and edges are folded from the log, both in log order.
pub struct Map {
    pub id: MapId,
    pub kind: String,
    pub path: PathBuf,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// One item on a map, added by a `NodeAdded` event.
pub struct Node {
    pub id: NodeId,
    pub kind: String,
    pub title: String,
    pub properties: BTreeMap<String, String>,
    pub references: Vec<Reference>,
    /// Every event that reached the node, the addition first, in log order.
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reference {
    Message { event: EventId, quote: String },
    File { path: PathBuf, lines: (u32, u32), excerpt: String },
}

/// A directed link between two nodes of a map, added by an `EdgeAdded` event and dropped by an `EdgeRemoved` one.
pub struct Edge {
    pub kind: String,
    pub from: NodeId,
    pub to: NodeId,
    /// Every event that reached the edge, the addition first, in log order.
    pub changes: Vec<Change>,
}

impl Edge {
    /// An edge is its kind and its two ends; two edges alike would be one fact stated twice.
    pub fn is(&self, kind: &str, from: NodeId, to: NodeId) -> bool {
        self.kind == kind && self.from == from && self.to == to
    }
}

#[cfg(test)]
impl Map {
    /// The map `id` after `events`, created by a `MapCreated` event placed first.
    pub fn fixture(id: MapId, events: &[Event]) -> Self {
        let created = Event::map_created(crate::core::Source::fixture(), Actor::Human, id, "concepts");
        let log: Vec<Event> = std::iter::once(created).chain(events.iter().cloned()).collect();

        Self::all(&log).remove(0)
    }
}

/// Who wrote to a node or an edge, and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub actor: Actor,
    pub at: Timestamp,
}

impl Map {
    /// Every map in `events` in `MapCreated` order, each after the events that reached it.
    /// Events of a map with no `MapCreated` are ignored.
    pub fn all(events: &[Event]) -> Vec<Self> {
        let mut maps: Vec<Self> = Vec::new();

        for event in events {
            match &event.payload {
                Payload::MapCreated { map, kind } => maps.push(Self {
                    id: *map,
                    kind: kind.clone(),
                    path: event.source.path.clone(),
                    nodes: Vec::new(),
                    edges: Vec::new(),
                }),
                payload => {
                    let id = payload.map();
                    if let Some(found) = maps.iter_mut().find(|m| Some(m.id) == id) {
                        found.apply(event);
                    }
                }
            }
        }

        maps
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Refuses an event that would break the map: a node title blank or held by another node,
    /// a node or an edge end that is not on the map, an edge the map already has or lacks.
    pub fn check(&self, event: &Event) -> Result<()> {
        match &event.payload {
            Payload::NodeAdded { title, .. } => self.check_title(title, None),
            Payload::NodeChanged { node, title, .. } => {
                self.check_node(*node)?;

                self.check_title(title, Some(*node))
            }
            Payload::NodeRemoved { node, .. } => self.check_node(*node),
            Payload::EdgeAdded { kind, from, to, .. } => {
                self.check_node(*from)?;
                self.check_node(*to)?;

                if self.edges.iter().any(|e| e.is(kind, *from, *to)) {
                    return Err(format!("the {} map already has this {kind} edge", self.kind).into());
                }

                Ok(())
            }
            Payload::EdgeRemoved { kind, from, to, .. } => {
                if !self.edges.iter().any(|e| e.is(kind, *from, *to)) {
                    return Err(format!("the {} map has no such {kind} edge", self.kind).into());
                }

                Ok(())
            }
            Payload::MapCreated { .. }
            | Payload::SessionCreated { .. }
            | Payload::SessionStarted { .. }
            | Payload::SessionStopped { .. }
            | Payload::Message { .. }
            | Payload::ToolUsed { .. }
            | Payload::SubagentStarted { .. }
            | Payload::SubagentReported { .. } => Ok(()),
        }
    }

    fn check_node(&self, id: NodeId) -> Result<()> {
        if self.node(id).is_none() {
            return Err(format!("the {} map has no such node", self.kind).into());
        }

        Ok(())
    }

    /// A title is not blank and is held by no node but `own`.
    fn check_title(&self, title: &str, own: Option<NodeId>) -> Result<()> {
        if title.trim().is_empty() {
            return Err("title must not be blank".into());
        }
        if self.nodes.iter().any(|n| Some(n.id) != own && n.title == title) {
            return Err(format!("the {} map already has a node titled {title:?}", self.kind).into());
        }

        Ok(())
    }

    /// Folds `event` in; changes to a node not on the map, and removals of a node or edge not on it, change nothing.
    /// A removed node takes every edge from or to it.
    fn apply(&mut self, event: &Event) {
        let change = Change { actor: event.actor, at: event.created_at };

        match &event.payload {
            Payload::NodeAdded { node, kind, title, properties, references, .. } => self.nodes.push(Node {
                id: *node,
                kind: kind.clone(),
                title: title.clone(),
                properties: properties.clone(),
                references: references.clone(),
                changes: vec![change],
            }),
            Payload::NodeChanged { node, title: new_title, properties: new_properties, references: new_references, .. } => {
                if let Some(found) = self.nodes.iter_mut().find(|n| n.id == *node) {
                    found.title = new_title.clone();
                    found.properties = new_properties.clone();
                    found.references = new_references.clone();
                    found.changes.push(change);
                }
            }
            Payload::NodeRemoved { node, .. } => {
                self.nodes.retain(|n| n.id != *node);
                self.edges.retain(|e| e.from != *node && e.to != *node);
            }
            Payload::EdgeAdded { kind, from, to, .. } => {
                self.edges.push(Edge { kind: kind.clone(), from: *from, to: *to, changes: vec![change] })
            }
            Payload::EdgeRemoved { kind, from, to, .. } => self.edges.retain(|e| !e.is(kind, *from, *to)),
            Payload::MapCreated { .. }
            | Payload::SessionCreated { .. }
            | Payload::SessionStarted { .. }
            | Payload::SessionStopped { .. }
            | Payload::Message { .. }
            | Payload::ToolUsed { .. }
            | Payload::SubagentStarted { .. }
            | Payload::SubagentReported { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::core::Source;

    fn node_added(map: MapId, actor: Actor, title: &str) -> (NodeId, Event) {
        let node = NodeId::new();
        let event = Event::node_added(Source::fixture(), actor, map, node, "concept", title, BTreeMap::new(), Vec::new());

        (node, event)
    }

    #[test]
    fn nodes_and_edges_are_the_maps_own_in_log_order() {
        let (map, other) = (MapId::new(), MapId::new());
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let (b, add_b) = node_added(map, Actor::Agent, "B");
        let (_, add_elsewhere) = node_added(other, Actor::Human, "Elsewhere");
        let events = [
            add_a,
            add_elsewhere,
            Event::edge_added(Source::fixture(), Actor::Human, other, "links", a, b),
            add_b,
            Event::edge_added(Source::fixture(), Actor::Human, map, "links", b, a),
            Event::edge_added(Source::fixture(), Actor::Human, map, "cites", a, b),
        ];

        let folded = Map::fixture(map, &events);

        let nodes: Vec<&str> = folded.nodes.iter().map(|n| n.title.as_str()).collect();
        let edges: Vec<(&str, NodeId, NodeId)> = folded.edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(nodes, vec!["A", "B"]);
        assert_eq!(edges, vec![("links", b, a), ("cites", a, b)]);
    }

    #[test]
    fn the_first_change_carries_the_events_actor_and_time() {
        let map = MapId::new();
        let (a, mut add_a) = node_added(map, Actor::Agent, "A");
        let mut add_edge = Event::edge_added(Source::fixture(), Actor::Human, map, "links", a, a);
        add_a.created_at = "2026-01-01T00:00:00Z".parse().unwrap();
        add_edge.created_at = "2026-01-02T00:00:00Z".parse().unwrap();

        let folded = Map::fixture(map, &[add_a.clone(), add_edge.clone()]);

        assert_eq!(folded.nodes[0].changes, vec![Change { actor: Actor::Agent, at: add_a.created_at }]);
        assert_eq!(folded.edges[0].changes, vec![Change { actor: Actor::Human, at: add_edge.created_at }]);
    }

    #[test]
    fn a_change_replaces_the_nodes_state_keeps_its_place_and_appends_a_change() {
        let map = MapId::new();
        let (a, add_a) = node_added(map, Actor::Agent, "A");
        let (_, add_b) = node_added(map, Actor::Agent, "B");
        let references = vec![Reference::Message { event: EventId::new(), quote: "Q".into() }];
        let properties = BTreeMap::from([("definition".to_string(), "D".to_string())]);
        let mut change = Event::node_changed(Source::fixture(), Actor::Human, map, a, "A2", properties.clone(), references.clone());
        change.created_at = "2026-01-03T00:00:00Z".parse().unwrap();

        let folded = Map::fixture(map, &[add_a.clone(), add_b, change.clone()]);

        let titles: Vec<&str> = folded.nodes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["A2", "B"]);
        assert_eq!(folded.nodes[0].id, a);
        assert_eq!(folded.nodes[0].kind, "concept");
        assert_eq!(folded.nodes[0].properties, properties);
        assert_eq!(folded.nodes[0].references, references);
        assert_eq!(folded.nodes[0].changes, vec![
            Change { actor: Actor::Agent, at: add_a.created_at },
            Change { actor: Actor::Human, at: change.created_at },
        ]);
        assert_eq!(folded.nodes[1].changes.len(), 1);
    }

    #[test]
    fn a_change_to_another_maps_or_a_missing_node_changes_nothing() {
        let (map, other) = (MapId::new(), MapId::new());
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let elsewhere = Event::node_changed(Source::fixture(), Actor::Human, other, a, "X", BTreeMap::new(), Vec::new());
        let missing = Event::node_changed(Source::fixture(), Actor::Human, map, NodeId::new(), "Y", BTreeMap::new(), Vec::new());

        let folded = Map::fixture(map, &[add_a, elsewhere, missing]);

        assert_eq!(folded.nodes.len(), 1);
        assert_eq!(folded.nodes[0].title, "A");
        assert_eq!(folded.nodes[0].changes.len(), 1);
    }

    #[test]
    fn a_removal_drops_the_matching_edge_and_nothing_else() {
        let (map, other) = (MapId::new(), MapId::new());
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let (b, add_b) = node_added(map, Actor::Human, "B");
        let edge = |map, kind, from, to| Event::edge_added(Source::fixture(), Actor::Human, map, kind, from, to);
        let remove = |map, kind, from, to| Event::edge_removed(Source::fixture(), Actor::Agent, map, kind, from, to);
        let events = [
            add_a,
            add_b,
            edge(map, "links", a, b),
            edge(map, "links", b, a),
            edge(map, "cites", a, b),
            remove(other, "links", a, b),
            remove(map, "links", a, a),
            remove(map, "links", a, b),
            remove(map, "links", a, b),
        ];

        let folded = Map::fixture(map, &events);

        let edges: Vec<(&str, NodeId, NodeId)> = folded.edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(edges, vec![("links", b, a), ("cites", a, b)]);
        assert_eq!(folded.nodes.len(), 2);
    }

    #[test]
    fn a_removed_node_takes_its_edges_in_both_directions_and_nothing_else() {
        let (map, other) = (MapId::new(), MapId::new());
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let (b, add_b) = node_added(map, Actor::Human, "B");
        let (c, add_c) = node_added(map, Actor::Human, "C");
        let (elsewhere, add_elsewhere) = node_added(other, Actor::Human, "A");
        let edge = |map, from, to| Event::edge_added(Source::fixture(), Actor::Human, map, "links", from, to);
        let events = [
            add_a,
            add_b,
            add_c,
            add_elsewhere,
            edge(map, a, b),
            edge(map, c, a),
            edge(map, b, c),
            edge(other, elsewhere, elsewhere),
            Event::node_removed(Source::fixture(), Actor::Agent, map, a),
            Event::node_removed(Source::fixture(), Actor::Agent, other, b),
        ];

        let created = |id| Event::map_created(Source::fixture(), Actor::Human, id, "concepts");
        let log: Vec<Event> = [created(map), created(other)].into_iter().chain(events).collect();

        let maps = Map::all(&log);
        let (folded, elsewhere_map) = (&maps[0], &maps[1]);

        let titles: Vec<&str> = folded.nodes.iter().map(|n| n.title.as_str()).collect();
        let edges: Vec<(NodeId, NodeId)> = folded.edges.iter().map(|e| (e.from, e.to)).collect();
        assert_eq!(titles, vec!["B", "C"]);
        assert_eq!(edges, vec![(b, c)]);
        assert_eq!(elsewhere_map.nodes.len(), 1);
        assert_eq!(elsewhere_map.edges.len(), 1);
    }

    #[test]
    fn removing_a_missing_node_changes_nothing() {
        let map = MapId::new();
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let events = [
            add_a,
            Event::edge_added(Source::fixture(), Actor::Human, map, "links", a, a),
            Event::node_removed(Source::fixture(), Actor::Agent, map, NodeId::new()),
        ];

        let folded = Map::fixture(map, &events);

        assert_eq!(folded.nodes.len(), 1);
        assert_eq!(folded.edges.len(), 1);
    }

    #[test]
    fn a_removed_node_added_again_is_a_new_node() {
        let map = MapId::new();
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let (again, add_again) = node_added(map, Actor::Agent, "A");
        let events = [add_a, Event::node_removed(Source::fixture(), Actor::Human, map, a), add_again];

        let folded = Map::fixture(map, &events);

        assert_eq!(folded.nodes.len(), 1);
        assert_eq!(folded.nodes[0].id, again);
        assert_eq!(folded.nodes[0].changes.len(), 1);
    }

    #[test]
    fn a_removed_edge_added_again_starts_a_new_history() {
        let map = MapId::new();
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let events = [
            add_a,
            Event::edge_added(Source::fixture(), Actor::Human, map, "links", a, a),
            Event::edge_removed(Source::fixture(), Actor::Human, map, "links", a, a),
            Event::edge_added(Source::fixture(), Actor::Agent, map, "links", a, a),
        ];

        let folded = Map::fixture(map, &events);

        assert_eq!(folded.edges.len(), 1);
        assert_eq!(folded.edges[0].changes.len(), 1);
        assert_eq!(folded.edges[0].changes[0].actor, Actor::Agent);
    }

    #[test]
    fn a_node_keeps_its_references() {
        let map = MapId::new();
        let references = vec![
            Reference::Message { event: EventId::new(), quote: "Keep the text.".into() },
            Reference::File { path: "src/main.rs".into(), lines: (1, 3), excerpt: "fn main() {}".into() },
        ];
        let event = Event::node_added(
            Source::fixture(), Actor::Human, map, NodeId::new(),
            "decision", "Keep snapshots", BTreeMap::new(), references.clone(),
        );

        let folded = Map::fixture(map, &[event]);

        assert_eq!(folded.nodes[0].references, references);
    }

    #[test]
    fn one_fold_builds_each_map_in_creation_order_and_ignores_a_map_never_created() {
        let (first, second, uncreated) = (MapId::new(), MapId::new(), MapId::new());
        let (a, add_a) = node_added(first, Actor::Human, "A");
        let (b, add_b) = node_added(second, Actor::Agent, "B");
        let (_, add_lost) = node_added(uncreated, Actor::Human, "Lost");
        let created = |id, kind| Event::map_created(Source::fixture(), Actor::Human, id, kind);
        let events = [
            created(first, "concepts"),
            add_lost,
            add_a,
            created(second, "decisions"),
            Event::edge_added(Source::fixture(), Actor::Human, first, "links", a, a),
            add_b,
            Event::edge_added(Source::fixture(), Actor::Agent, second, "links", b, b),
            Event::edge_added(Source::fixture(), Actor::Agent, uncreated, "links", a, b),
        ];

        let maps = Map::all(&events);

        let found: Vec<(MapId, &str, &Path)> = maps.iter().map(|m| (m.id, m.kind.as_str(), m.path.as_path())).collect();
        assert_eq!(found, vec![(first, "concepts", Path::new("/repo")), (second, "decisions", Path::new("/repo"))]);
        assert_eq!(maps[0].nodes.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), vec!["A"]);
        assert_eq!(maps[0].edges.iter().map(|e| (e.from, e.to)).collect::<Vec<_>>(), vec![(a, a)]);
        assert_eq!(maps[1].nodes.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), vec!["B"]);
        assert_eq!(maps[1].edges.iter().map(|e| (e.from, e.to)).collect::<Vec<_>>(), vec![(b, b)]);
    }

    fn check_map() -> (MapId, NodeId, NodeId, Map) {
        let map = MapId::new();
        let (a, add_a) = node_added(map, Actor::Human, "A");
        let (b, add_b) = node_added(map, Actor::Human, "B");
        let edge = Event::edge_added(Source::fixture(), Actor::Human, map, "links", a, b);

        (map, a, b, Map::fixture(map, &[add_a, add_b, edge]))
    }

    #[test]
    fn check_refuses_a_node_added_with_a_blank_title_or_one_another_node_holds() {
        let (map, _, _, folded) = check_map();

        let blank = folded.check(&node_added(map, Actor::Agent, " ").1);
        let taken = folded.check(&node_added(map, Actor::Agent, "A").1);
        let free = folded.check(&node_added(map, Actor::Agent, "C").1);

        assert_eq!(blank.unwrap_err().to_string(), "title must not be blank");
        assert_eq!(taken.unwrap_err().to_string(), "the concepts map already has a node titled \"A\"");
        assert!(free.is_ok());
    }

    fn change(map: MapId, node: NodeId, title: &str) -> Event {
        Event::node_changed(Source::fixture(), Actor::Human, map, node, title, BTreeMap::new(), Vec::new())
    }

    #[test]
    fn check_refuses_a_node_changed_that_is_missing_or_gets_a_blank_or_taken_title() {
        let (map, a, _, folded) = check_map();

        let missing = folded.check(&change(map, NodeId::new(), "C"));
        let blank = folded.check(&change(map, a, " "));
        let taken = folded.check(&change(map, a, "B"));
        let renamed = folded.check(&change(map, a, "C"));

        assert_eq!(missing.unwrap_err().to_string(), "the concepts map has no such node");
        assert_eq!(blank.unwrap_err().to_string(), "title must not be blank");
        assert_eq!(taken.unwrap_err().to_string(), "the concepts map already has a node titled \"B\"");
        assert!(renamed.is_ok());
    }

    #[test]
    fn check_lets_a_node_keep_its_own_title_on_a_change() {
        let (map, a, _, folded) = check_map();

        let result = folded.check(&change(map, a, "A"));

        assert!(result.is_ok());
    }

    #[test]
    fn check_refuses_a_node_removed_that_is_not_on_the_map() {
        let (map, a, _, folded) = check_map();

        let missing = folded.check(&Event::node_removed(Source::fixture(), Actor::Human, map, NodeId::new()));
        let present = folded.check(&Event::node_removed(Source::fixture(), Actor::Human, map, a));

        assert_eq!(missing.unwrap_err().to_string(), "the concepts map has no such node");
        assert!(present.is_ok());
    }

    #[test]
    fn check_refuses_an_edge_added_with_an_end_off_the_map_or_one_the_map_has() {
        let (map, a, b, folded) = check_map();
        let edge = |kind, from, to| Event::edge_added(Source::fixture(), Actor::Human, map, kind, from, to);

        let from_missing = folded.check(&edge("links", NodeId::new(), b));
        let to_missing = folded.check(&edge("links", a, NodeId::new()));
        let repeated = folded.check(&edge("links", a, b));
        let reversed = folded.check(&edge("links", b, a));
        let other_kind = folded.check(&edge("cites", a, b));

        assert_eq!(from_missing.unwrap_err().to_string(), "the concepts map has no such node");
        assert_eq!(to_missing.unwrap_err().to_string(), "the concepts map has no such node");
        assert_eq!(repeated.unwrap_err().to_string(), "the concepts map already has this links edge");
        assert!(reversed.is_ok());
        assert!(other_kind.is_ok());
    }

    #[test]
    fn check_refuses_an_edge_removed_that_is_not_on_the_map() {
        let (map, a, b, folded) = check_map();
        let remove = |kind, from, to| Event::edge_removed(Source::fixture(), Actor::Human, map, kind, from, to);

        let missing = folded.check(&remove("links", b, a));
        let other_kind = folded.check(&remove("cites", a, b));
        let present = folded.check(&remove("links", a, b));

        assert_eq!(missing.unwrap_err().to_string(), "the concepts map has no such links edge");
        assert!(other_kind.is_err());
        assert!(present.is_ok());
    }

    #[test]
    fn check_passes_every_other_payload() {
        let (map, _, _, folded) = check_map();
        let session = crate::core::SessionId::new();
        let events = [
            Event::map_created(Source::fixture(), Actor::Human, map, "concepts"),
            Event::session_created(Source::fixture(), session, crate::core::ClientSessionId::new("c".into())),
            Event::message(Source::fixture(), session, Actor::Human, "Hello", serde_json::json!({})),
        ];

        for event in &events {
            assert!(folded.check(event).is_ok());
        }
    }
}
