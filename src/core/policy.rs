use super::{Actor, Change, Edge, Event, Map, Payload};
use crate::shared::Result;

/// Who may make a write. It judges new events only, never the log, so a later rule cannot invalidate it.
pub struct Policy;

const CHECKED: &str = "Map::check refuses a write to what is not on the map";

impl Policy {
    /// Refuses an agent's change to a node, or removal of a node or an edge, that a human wrote.
    /// Removing a node also removes its edges, so a human's edge on it refuses the removal. Additions and a human's writes pass.
    /// The event has passed `Map::check`, so what it changes or removes is on the map.
    pub fn check(event: &Event, map: &Map) -> Result<()> {
        if event.actor != Actor::Agent {
            return Ok(());
        }

        match &event.payload {
            Payload::NodeChanged { node, .. } => {
                let found = map.node(*node).expect(CHECKED);

                refuse_human(&found.changes, &format!("{:?}", found.title), "change")
            }
            Payload::EdgeRemoved { kind, from, to, .. } => {
                let found = map.edges.iter().find(|e| e.is(kind, *from, *to)).expect(CHECKED);

                refuse_human(&found.changes, &edge_name(map, found), "remove")
            }
            Payload::NodeRemoved { node, .. } => {
                let found = map.node(*node).expect(CHECKED);

                refuse_human(&found.changes, &format!("{:?}", found.title), "remove")?;

                let cascade = format!("remove {:?}, which removes", found.title);
                map.edges
                    .iter()
                    .filter(|e| e.from == *node || e.to == *node)
                    .try_for_each(|e| refuse_human(&e.changes, &edge_name(map, e), &cascade))
            }
            Payload::MapCreated { .. }
            | Payload::NodeAdded { .. }
            | Payload::EdgeAdded { .. }
            | Payload::SessionCreated { .. }
            | Payload::SessionStarted { .. }
            | Payload::SessionStopped { .. }
            | Payload::Message { .. }
            | Payload::ToolUsed { .. }
            | Payload::SubagentStarted { .. }
            | Payload::SubagentReported { .. } => Ok(()),
        }
    }
}

fn edge_name(map: &Map, edge: &Edge) -> String {
    let title = |id| map.node(id).map_or("", |n| n.title.as_str());

    format!("the edge {:?} {} {:?}", title(edge.from), edge.kind, title(edge.to))
}

/// Refuses when any of `changes` is a human's: `what` was written by a human, so only a human may `verb` it.
fn refuse_human(changes: &[Change], what: &str, verb: &str) -> Result<()> {
    if changes.iter().any(|c| c.actor == Actor::Human) {
        return Err(format!("{what} was written by a human, so only a human may {verb} it").into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::core::{MapId, NodeId, Source};

    fn added(map: MapId, actor: Actor, title: &str) -> (NodeId, Event) {
        let node = NodeId::new();
        let event = Event::node_added(Source::fixture(), actor, map, node, "concept", title, BTreeMap::new(), Vec::new());

        (node, event)
    }

    fn change(map: MapId, actor: Actor, node: NodeId) -> Event {
        Event::node_changed(Source::fixture(), actor, map, node, "New", BTreeMap::new(), Vec::new())
    }

    #[test]
    fn an_agent_may_not_change_a_node_a_human_added() {
        let map = MapId::new();
        let (node, add) = added(map, Actor::Human, "A");
        let map_ = Map::fixture(map, &[add]);

        let result = Policy::check(&change(map, Actor::Agent, node), &map_);

        let error = result.unwrap_err().to_string();
        assert!(error.contains("\"A\""));
        assert!(error.contains("only a human may change it"));
    }

    #[test]
    fn an_agent_may_not_change_an_agents_node_a_human_later_changed() {
        let map = MapId::new();
        let (node, add) = added(map, Actor::Agent, "A");
        let map_ = Map::fixture(map, &[add, change(map, Actor::Human, node)]);

        let result = Policy::check(&change(map, Actor::Agent, node), &map_);

        assert!(result.is_err());
    }

    #[test]
    fn an_agent_may_change_a_node_only_agents_wrote() {
        let map = MapId::new();
        let (node, add) = added(map, Actor::Agent, "A");
        let map_ = Map::fixture(map, &[add, change(map, Actor::Agent, node)]);

        let result = Policy::check(&change(map, Actor::Agent, node), &map_);

        assert!(result.is_ok());
    }

    #[test]
    fn a_human_may_change_an_agents_node() {
        let map = MapId::new();
        let (node, add) = added(map, Actor::Agent, "A");
        let map_ = Map::fixture(map, &[add]);

        let result = Policy::check(&change(map, Actor::Human, node), &map_);

        assert!(result.is_ok());
    }

    fn edge_removal(map: MapId, actor: Actor, from: NodeId, to: NodeId) -> Event {
        Event::edge_removed(Source::fixture(), actor, map, "links", from, to)
    }

    fn linked(map: MapId, edge_actor: Actor) -> (NodeId, NodeId, Map) {
        let (a, add_a) = added(map, Actor::Agent, "A");
        let (b, add_b) = added(map, Actor::Agent, "B");
        let edge = Event::edge_added(Source::fixture(), edge_actor, map, "links", a, b);
        let map_ = Map::fixture(map, &[add_a, add_b, edge]);

        (a, b, map_)
    }

    #[test]
    fn an_agent_may_not_remove_an_edge_a_human_added() {
        let map = MapId::new();
        let (a, b, map_) = linked(map, Actor::Human);

        let result = Policy::check(&edge_removal(map, Actor::Agent, a, b), &map_);

        let error = result.unwrap_err().to_string();
        assert!(error.contains("\"A\" links \"B\""));
        assert!(error.contains("written by a human, so only a human may remove it"));
    }

    #[test]
    fn an_agent_may_remove_an_edge_only_agents_wrote() {
        let map = MapId::new();
        let (a, b, map_) = linked(map, Actor::Agent);

        let result = Policy::check(&edge_removal(map, Actor::Agent, a, b), &map_);

        assert!(result.is_ok());
    }

    #[test]
    fn a_human_may_remove_an_agents_edge() {
        let map = MapId::new();
        let (a, b, map_) = linked(map, Actor::Agent);

        let result = Policy::check(&edge_removal(map, Actor::Human, a, b), &map_);

        assert!(result.is_ok());
    }

    fn node_removal(map: MapId, actor: Actor, node: NodeId) -> Event {
        Event::node_removed(Source::fixture(), actor, map, node)
    }

    #[test]
    fn an_agent_may_not_remove_a_node_a_human_added() {
        let map = MapId::new();
        let (node, add) = added(map, Actor::Human, "A");
        let map_ = Map::fixture(map, &[add]);

        let result = Policy::check(&node_removal(map, Actor::Agent, node), &map_);

        let error = result.unwrap_err().to_string();
        assert!(error.contains("\"A\" was written by a human, so only a human may remove it"));
    }

    #[test]
    fn an_agent_may_not_remove_its_node_when_a_human_changed_it() {
        let map = MapId::new();
        let (node, add) = added(map, Actor::Agent, "A");
        let map_ = Map::fixture(map, &[add, change(map, Actor::Human, node)]);

        let result = Policy::check(&node_removal(map, Actor::Agent, node), &map_);

        assert!(result.is_err());
    }

    #[test]
    fn an_agent_may_not_remove_its_node_with_a_human_edge_either_way() {
        let map = MapId::new();
        for outgoing in [true, false] {
            let (a, add_a) = added(map, Actor::Agent, "A");
            let (b, add_b) = added(map, Actor::Agent, "B");
            let (from, to) = if outgoing { (a, b) } else { (b, a) };
            let edge = Event::edge_added(Source::fixture(), Actor::Human, map, "links", from, to);
            let map_ = Map::fixture(map, &[add_a, add_b, edge]);

            let result = Policy::check(&node_removal(map, Actor::Agent, a), &map_);

            let error = result.unwrap_err().to_string();
            let named = if outgoing { "\"A\" links \"B\"" } else { "\"B\" links \"A\"" };
            assert!(error.contains(named), "{error}");
            assert!(error.contains("only a human may remove \"A\", which removes it"), "{error}");
        }
    }

    #[test]
    fn an_agent_may_remove_its_node_with_only_agents_edges() {
        let map = MapId::new();
        let (a, b, map_) = linked(map, Actor::Agent);

        let result = Policy::check(&node_removal(map, Actor::Agent, a), &map_);

        assert!(result.is_ok());
        assert!(Policy::check(&node_removal(map, Actor::Agent, b), &map_).is_ok());
    }

    #[test]
    fn a_human_may_remove_a_node_with_a_human_edge() {
        let map = MapId::new();
        let (a, _, map_) = linked(map, Actor::Human);

        let result = Policy::check(&node_removal(map, Actor::Human, a), &map_);

        assert!(result.is_ok());
    }

    #[test]
    fn additions_pass() {
        let map = MapId::new();
        let (a, add_a) = added(map, Actor::Human, "A");
        let (_, add_b) = added(map, Actor::Agent, "B");
        let map_ = Map::fixture(map, &[add_a]);
        let edge = Event::edge_added(Source::fixture(), Actor::Agent, map, "links", a, a);

        assert!(Policy::check(&add_b, &map_).is_ok());
        assert!(Policy::check(&edge, &map_).is_ok());
    }
}
