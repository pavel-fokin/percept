use std::collections::BTreeMap;
use std::path::Path;

use super::AppService;
use crate::core::{Actor, Edge, Event, EventStore, Map, MapId, Node, NodeId, Payload, SchemaStore, Source};
use crate::shared::Result;

/// A map with how many nodes and edges it holds.
pub struct MapSummary {
    pub map: Map,
    pub nodes: usize,
    pub edges: usize,
}

/// A map with its nodes, newest first, and its edges in log order.
pub struct MapDetail {
    pub map: Map,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl<S: EventStore, M: SchemaStore> AppService<S, M> {
    /// Records a new map. A project holds at most one map of each kind.
    pub async fn create_map(
        &self,
        source: Source,
        actor: Actor,
        kind: &str,
    ) -> Result<()> {
        not_blank("kind", kind)?;
        self.schemas.schema(&source.path, kind).await?;

        // Held until return, so parallel creates cannot both pass the check.
        let _lock = self.store.lock().await?;

        let taken = self.maps(&source.path).await?.iter().any(|m| m.kind == kind);
        if taken {
            return Err(format!("this project already has a {kind} map").into());
        }

        self.store.append(&Event::map_created(source, actor, MapId::new(), kind)).await
    }

    /// The project's maps in log order.
    pub async fn maps(&self, project: &Path) -> Result<Vec<Map>> {
        Ok(all_maps(&self.store.all().await?).into_iter().filter(|m| m.path == project).collect())
    }

    /// The project's map of the given kind, or an error if it has none.
    pub async fn map(&self, project: &Path, kind: &str) -> Result<Map> {
        let found = self.maps(project).await?.into_iter().find(|m| m.kind == kind);

        found.ok_or_else(|| format!("this project has no {kind} map").into())
    }

    /// The map's nodes in log order.
    pub async fn nodes(&self, map: MapId) -> Result<Vec<Node>> {
        Ok(map_nodes(&self.store.all().await?, map))
    }

    /// The project's maps in log order, each with its node and edge counts, from one read of the log.
    pub async fn map_summaries(&self, project: &Path) -> Result<Vec<MapSummary>> {
        let mut summaries = self.all_map_summaries().await?;
        summaries.retain(|s| s.map.path == project);

        Ok(summaries)
    }

    /// Every map across all projects in log order, each with its node and edge counts, from one read of the log.
    pub async fn all_map_summaries(&self) -> Result<Vec<MapSummary>> {
        let events = self.store.all().await?;

        let mut summaries: Vec<MapSummary> = all_maps(&events)
            .into_iter()
            .map(|map| MapSummary { map, nodes: 0, edges: 0 })
            .collect();

        for e in &events {
            match &e.payload {
                Payload::NodeAdded { map, .. } => summaries.iter_mut().filter(|s| s.map.id == *map).for_each(|s| s.nodes += 1),
                Payload::EdgeAdded { map, .. } => summaries.iter_mut().filter(|s| s.map.id == *map).for_each(|s| s.edges += 1),
                _ => {}
            }
        }

        Ok(summaries)
    }

    /// The map with the given id, its nodes newest first and its edges in log order, from one read of the log.
    pub async fn map_detail(&self, id: MapId) -> Result<Option<MapDetail>> {
        let events = self.store.all().await?;

        let Some(map) = all_maps(&events).into_iter().find(|m| m.id == id) else {
            return Ok(None);
        };

        let mut nodes = map_nodes(&events, id);
        nodes.reverse();

        Ok(Some(MapDetail { map, nodes, edges: map_edges(&events, id) }))
    }

    /// The map's edges in log order.
    pub async fn edges(&self, map: MapId) -> Result<Vec<Edge>> {
        Ok(map_edges(&self.store.all().await?, map))
    }

    /// Records a new edge on the project's map of the given kind, between the nodes with these titles.
    pub async fn add_edge(
        &self,
        source: Source,
        actor: Actor,
        map_kind: &str,
        from: &str,
        kind: &str,
        to: &str,
    ) -> Result<()> {
        not_blank("kind", kind)?;

        // Held until return, so the nodes cannot change between the check and the append.
        let _lock = self.store.lock().await?;

        let map = self.map(&source.path, map_kind).await?;
        let nodes = self.nodes(map.id).await?;

        let find = |title: &str| {
            nodes
                .iter()
                .find(|n| n.title == title)
                .map(|n| n.id)
                .ok_or_else(|| format!("the {map_kind} map has no node titled {title:?}"))
        };
        let (from, to) = (find(from)?, find(to)?);

        let repeated = self.edges(map.id).await?.iter().any(|e| e.from == from && e.kind == kind && e.to == to);
        if repeated {
            return Err(format!("the {map_kind} map already has this {kind} edge").into());
        }

        self.store.append(&Event::edge_added(source, actor, map.id, kind, from, to)).await
    }

    /// Records a new node on the project's map of the given kind. A title is unique within its map.
    pub async fn add_node(
        &self,
        source: Source,
        actor: Actor,
        map_kind: &str,
        kind: &str,
        title: &str,
        properties: BTreeMap<String, String>,
    ) -> Result<()> {
        not_blank("kind", kind)?;
        not_blank("title", title)?;

        // Held until return, so parallel adds cannot both pass the check.
        let _lock = self.store.lock().await?;

        let map = self.map(&source.path, map_kind).await?;
        self.schemas.schema(&source.path, map_kind).await?.check_node(kind, &properties)?;
        if self.nodes(map.id).await?.iter().any(|n| n.title == title) {
            return Err(format!("the {map_kind} map already has a node titled {title:?}").into());
        }

        self.store.append(&Event::node_added(source, actor, map.id, NodeId::new(), kind, title, properties)).await
    }
}

fn not_blank(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be blank").into());
    }

    Ok(())
}

fn all_maps(events: &[Event]) -> Vec<Map> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            Payload::MapCreated { map, kind } => Some(Map { id: *map, kind: kind.clone(), path: e.source.path.clone() }),
            _ => None,
        })
        .collect()
}

fn map_nodes(events: &[Event], map: MapId) -> Vec<Node> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            Payload::NodeAdded { map: id, node, kind, title, properties } if *id == map => Some(Node {
                id: *node,
                kind: kind.clone(),
                title: title.clone(),
                properties: properties.clone(),
                created_at: e.created_at,
            }),
            _ => None,
        })
        .collect()
}

fn map_edges(events: &[Event], map: MapId) -> Vec<Edge> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            Payload::EdgeAdded { map: id, kind, from, to } if *id == map => Some(Edge { kind: kind.clone(), from: *from, to: *to }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Memory, NoSchemas, Schemas};

    #[tokio::test]
    async fn the_same_map_kind_can_be_created_in_two_projects_but_not_twice_in_one() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        let other = Source { client: "claude".into(), path: "/other".into() };
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(other, Actor::Human, "decisions").await.unwrap();

        let again = service.create_map(Source::fixture(), Actor::Agent, "decisions").await;

        assert!(again.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_map_is_created_only_from_a_schema() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);

        let with_schema = service.create_map(Source::fixture(), Actor::Human, "decisions").await;
        let without_schema = service.create_map(Source::fixture(), Actor::Human, "nope").await;

        assert!(with_schema.is_ok());
        assert!(without_schema.unwrap_err().to_string().contains("nope"));
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn maps_are_the_current_projects_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        let other = Source { client: "claude".into(), path: "/other".into() };
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(other, Actor::Human, "tasks").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();

        let maps = service.maps(&Source::fixture().path).await.unwrap();

        let found: Vec<&str> = maps.iter().map(|m| m.kind.as_str()).collect();
        assert_eq!(found, vec!["decisions", "tasks"]);
    }

    #[tokio::test]
    async fn add_node_to_a_missing_map_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();

        let result = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default()).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_node_checks_its_kind_and_properties_against_the_schema() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let undeclared = BTreeMap::from([("colour".to_string(), "red".to_string())]);

        let unknown_kind = service.add_node(Source::fixture(), Actor::Human, "concepts", "idea", "A", Default::default()).await;
        let unknown_property = service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "B", undeclared).await;
        let subset = service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "C", Default::default()).await;

        assert!(unknown_kind.unwrap_err().to_string().contains("concept"));
        assert!(unknown_property.unwrap_err().to_string().contains("definition"));
        assert!(subset.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn add_node_fails_when_the_maps_schema_is_gone() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        let gone = AppService::new(&store, NoSchemas);

        let result = gone.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default()).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_node_records_its_properties() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let properties = BTreeMap::from([("definition".to_string(), "One log.".to_string())]);

        service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "Event log", properties.clone()).await.unwrap();

        let map = service.map(&Source::fixture().path, "concepts").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        assert_eq!(nodes[0].properties, properties);
    }

    #[tokio::test]
    async fn blank_kinds_and_titles_are_errors() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default()).await.unwrap();

        let map = service.create_map(Source::fixture(), Actor::Human, " ").await;
        let node = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "", Default::default()).await;
        let edge = service.add_edge(Source::fixture(), Actor::Human, "decisions", "Q", "", "Q").await;

        assert!(map.is_err());
        assert!(node.is_err());
        assert!(edge.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_repeated_edge_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let again = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await;
        let other_kind = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "blocks", "Q").await;

        assert!(again.is_err());
        assert!(other_kind.is_ok());
    }

    #[tokio::test]
    async fn a_node_title_is_unique_within_its_map_but_not_across_maps() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Why?", Default::default()).await.unwrap();

        let same_map = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "Why?", Default::default()).await;
        let other_map = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "Why?", Default::default()).await;

        assert!(same_map.is_err());
        assert!(other_map.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn nodes_are_the_maps_own_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "option", "O", Default::default()).await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let nodes = service.nodes(map.id).await.unwrap();

        let found: Vec<(&str, &str)> = nodes.iter().map(|n| (n.kind.as_str(), n.title.as_str())).collect();
        assert_eq!(found, vec![("question", "Q"), ("option", "O")]);
        assert!(service.map(&Source::fixture().path, "nope").await.is_err());
    }

    #[tokio::test]
    async fn an_edge_links_two_titled_nodes_and_a_missing_title_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default()).await.unwrap();

        let missing_to = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "nope").await;
        let missing_from = service.add_edge(Source::fixture(), Actor::Human, "decisions", "nope", "answers", "Q").await;
        let missing_map = service.add_edge(Source::fixture(), Actor::Human, "tasks", "O", "answers", "Q").await;
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        assert!(missing_to.unwrap_err().to_string().contains("\"nope\""));
        assert!(missing_from.is_err());
        assert!(missing_map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn edges_are_the_maps_own_in_log_order_and_name_node_ids() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        for (map, kind, title) in [("decisions", "question", "Q"), ("decisions", "option", "O"), ("tasks", "task", "T"), ("tasks", "task", "U")] {
            service.add_node(Source::fixture(), Actor::Human, map, kind, title, Default::default()).await.unwrap();
        }
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "tasks", "T", "blocks", "U").await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "Q", "raises", "O").await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();

        let edges = service.edges(map.id).await.unwrap();

        let found: Vec<(&str, NodeId, NodeId)> = edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(found, vec![("answers", nodes[1].id, nodes[0].id), ("raises", nodes[0].id, nodes[1].id)]);
    }

    #[tokio::test]
    async fn map_summaries_count_each_maps_nodes_and_edges() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let summaries = service.map_summaries(&Source::fixture().path).await.unwrap();

        let found: Vec<(&str, usize, usize)> = summaries.iter().map(|s| (s.map.kind.as_str(), s.nodes, s.edges)).collect();
        assert_eq!(found, vec![("decisions", 2, 1), ("tasks", 1, 0)]);
    }

    #[tokio::test]
    async fn all_map_summaries_span_projects_and_carry_each_maps_path() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        let other = Source { client: "claude".into(), path: "/other".into() };
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(other.clone(), Actor::Human, "tasks").await.unwrap();
        service.add_node(other, Actor::Human, "tasks", "task", "T", Default::default()).await.unwrap();

        let summaries = service.all_map_summaries().await.unwrap();

        let found: Vec<(&str, &Path, usize)> = summaries.iter().map(|s| (s.map.kind.as_str(), s.map.path.as_path(), s.nodes)).collect();
        assert_eq!(found, vec![("decisions", Source::fixture().path.as_path(), 0), ("tasks", Path::new("/other"), 1)]);
    }

    #[tokio::test]
    async fn map_detail_has_nodes_newest_first_and_edges_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let detail = service.map_detail(map.id).await.unwrap().unwrap();

        let titles: Vec<&str> = detail.nodes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["O", "Q"]);
        let edges: Vec<(&str, NodeId, NodeId)> = detail.edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(edges, vec![("answers", detail.nodes[0].id, detail.nodes[1].id)]);
        assert_eq!(detail.map.kind, "decisions");
    }

    #[tokio::test]
    async fn map_detail_of_an_unknown_id_is_none() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();

        let detail = service.map_detail(MapId::new()).await.unwrap();

        assert!(detail.is_none());
    }

    #[tokio::test]
    async fn create_map_passes_store_errors_through() {
        let result = AppService::new(Broken, Schemas).create_map(Source::fixture(), Actor::Human, "decisions").await;

        assert!(result.is_err());
    }

}
