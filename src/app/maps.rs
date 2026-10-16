use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::AppService;
use crate::core::{Actor, CITES, CitationSource, Edge, Event, EventId, EventStore, Map, MapId, Node, NodeBody, NodeId, Payload, SchemaStore, Source};
use crate::shared::Result;

/// A map with how many notes and edges it holds. Citations and `cites` edges are not counted.
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

/// A source a note cites. A file is named relative to the project root; its text is read when the note is added.
pub enum Cite {
    Message(String),
    File { path: PathBuf, lines: (u32, u32) },
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

        self.store.append(&[Event::map_created(source, actor, MapId::new(), kind)]).await
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

    /// The project's maps in log order, each with its note and edge counts, from one read of the log.
    pub async fn map_summaries(&self, project: &Path) -> Result<Vec<MapSummary>> {
        let mut summaries = self.all_map_summaries().await?;
        summaries.retain(|s| s.map.path == project);

        Ok(summaries)
    }

    /// Every map across all projects in log order, each with its note and edge counts, from one read of the log.
    pub async fn all_map_summaries(&self) -> Result<Vec<MapSummary>> {
        let events = self.store.all().await?;

        let mut summaries: Vec<MapSummary> = all_maps(&events)
            .into_iter()
            .map(|map| MapSummary { map, nodes: 0, edges: 0 })
            .collect();

        for e in &events {
            match &e.payload {
                Payload::NodeAdded { map, body: NodeBody::Note { .. }, .. } => summaries.iter_mut().filter(|s| s.map.id == *map).for_each(|s| s.nodes += 1),
                Payload::EdgeAdded { map, kind, .. } if kind != CITES => summaries.iter_mut().filter(|s| s.map.id == *map).for_each(|s| s.edges += 1),
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
                .find_map(|n| match &n.body {
                    NodeBody::Note { kind, title: t, .. } if t == title => Some((n.id, kind.as_str())),
                    _ => None,
                })
                .ok_or_else(|| format!("the {map_kind} map has no node titled {title:?}"))
        };
        let ((from, from_kind), (to, to_kind)) = (find(from)?, find(to)?);
        self.schemas.schema(&source.path, map_kind).await?.check_edge(kind, from_kind, to_kind)?;

        let repeated = self.edges(map.id).await?.iter().any(|e| e.from == from && e.kind == kind && e.to == to);
        if repeated {
            return Err(format!("the {map_kind} map already has this {kind} edge").into());
        }

        self.store.append(&[Event::edge_added(source, actor, map.id, kind, from, to)]).await
    }

    /// Records a new note on the project's map of the given kind, with a `cites` edge to a citation for each source cited.
    /// A title is unique among the notes of its map.
    #[allow(clippy::too_many_arguments, reason = "the arguments of `percept add`, one each")]
    pub async fn add_node(
        &self,
        source: Source,
        actor: Actor,
        map_kind: &str,
        kind: &str,
        title: &str,
        properties: BTreeMap<String, String>,
        cites: &[Cite],
    ) -> Result<()> {
        not_blank("kind", kind)?;
        not_blank("title", title)?;
        for cite in cites {
            if let Cite::Message(quote) = cite {
                not_blank("quote", quote)?;
            }
        }

        // Read before the lock, which hooks wait on.
        let mut resolved = Vec::new();
        for cite in cites {
            resolved.push(match cite {
                Cite::Message(quote) => Resolved::Message(quote),
                Cite::File { path, lines } => Resolved::Source(read_file(&source.path, path, *lines).await?),
            });
        }

        // Held until return, so parallel adds cannot both pass the check.
        let _lock = self.store.lock().await?;

        let map = self.map(&source.path, map_kind).await?;
        self.schemas.schema(&source.path, map_kind).await?.check_node(kind, &properties)?;

        let events = self.store.all().await?;
        let nodes = map_nodes(&events, map.id);
        let taken = nodes.iter().any(|n| matches!(&n.body, NodeBody::Note { title: t, .. } if t == title));
        if taken {
            return Err(format!("the {map_kind} map already has a node titled {title:?}").into());
        }

        let Citations { cited, created } = cite(&events, &source.path, &nodes, resolved)?;

        let note = NodeId::new();
        let body = NodeBody::Note { kind: kind.into(), title: title.into(), properties };

        let mut batch = vec![Event::node_added(source.clone(), actor, map.id, note, body)];
        for (id, citation) in created {
            batch.push(Event::node_added(source.clone(), actor, map.id, id, NodeBody::Citation(citation)));
        }
        for id in cited {
            batch.push(Event::edge_added(source.clone(), actor, map.id, CITES, note, id));
        }

        self.store.append(&batch).await
    }
}

fn not_blank(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be blank").into());
    }

    Ok(())
}

/// The citations a note points to, and those of them that are new to the map.
struct Citations {
    cited: Vec<NodeId>,
    created: Vec<(NodeId, CitationSource)>,
}

/// A cite whose file has been read. A quote still needs the log to find its message.
enum Resolved<'a> {
    Message(&'a str),
    Source(CitationSource),
}

/// Finds the source of each cite: the message a quote is a substring of, or the current lines of a file.
/// A citation with an equal source is reused.
fn cite(
    events: &[Event],
    project: &Path,
    nodes: &[Node],
    cites: Vec<Resolved<'_>>,
) -> Result<Citations> {
    let mut known: Vec<(NodeId, CitationSource)> = nodes
        .iter()
        .filter_map(|n| match &n.body {
            NodeBody::Citation(source) => Some((n.id, source.clone())),
            NodeBody::Note { .. } => None,
        })
        .collect();
    let mut cited = Vec::new();
    let mut created = Vec::new();

    for cite in cites {
        let source = match cite {
            Resolved::Message(quote) => CitationSource::Message { event: find_message(events, project, quote)?, quote: quote.into() },
            Resolved::Source(source) => source,
        };

        let id = match known.iter().find(|(_, known)| *known == source) {
            Some((id, _)) => *id,
            None => {
                let id = NodeId::new();
                known.push((id, source.clone()));
                created.push((id, source));
                id
            }
        };

        if !cited.contains(&id) {
            cited.push(id);
        }
    }

    Ok(Citations { cited, created })
}

/// The project's message that contains the quote. Human messages come before agent messages,
/// since an agent often repeats the user's words. Newer messages come before older ones.
fn find_message(events: &[Event], project: &Path, quote: &str) -> Result<EventId> {
    let messages: Vec<(&Event, &str)> = events
        .iter()
        .rev()
        .filter(|e| e.source.path == project)
        .filter_map(|e| match &e.payload {
            Payload::Message { content, .. } => Some((e, content.as_str())),
            _ => None,
        })
        .collect();

    [Actor::Human, Actor::Agent]
        .into_iter()
        .find_map(|actor| messages.iter().find(|(e, content)| e.actor == actor && content.contains(quote)))
        .map(|(e, _)| e.id)
        .ok_or_else(|| format!("no message in this project contains {quote:?}; copy the text verbatim from a message").into())
}

/// The lines of a file in the project, joined by newlines, under the file's path from the project root.
/// The file must resolve inside the project root, so every spelling of a path cites the same file.
async fn read_file(root: &Path, path: &Path, lines @ (from, to): (u32, u32)) -> Result<CitationSource> {
    let shown = path.display();

    if from == 0 || from > to {
        return Err(format!("lines {from}-{to} of {shown} are not a range: lines count from 1 and from must not exceed to").into());
    }

    if path.is_absolute() {
        return Err(format!("{shown} is absolute; name the file relative to the project root").into());
    }

    let root = tokio::fs::canonicalize(root).await?;
    let file = tokio::fs::canonicalize(root.join(path)).await.map_err(|e| format!("cannot read {shown}: {e}"))?;
    let relative = file.strip_prefix(&root).map_err(|_| format!("{shown} is outside the project root"))?;

    let text = tokio::fs::read_to_string(&file).await.map_err(|e| format!("cannot read {shown}: {e}"))?;
    let all: Vec<&str> = text.lines().collect();
    if to as usize > all.len() {
        return Err(format!("{shown} has {} lines; line {to} is beyond the end", all.len()).into());
    }

    Ok(CitationSource::File { path: relative.into(), lines, excerpt: all[from as usize - 1..to as usize].join("\n") })
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
            Payload::NodeAdded { map: id, node, body } if *id == map => Some(Node {
                id: *node,
                body: body.clone(),
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
    use crate::app::testing::{Broken, Memory, NoSchemas, Schemas, TempDir};
    use crate::core::SessionId;

    fn say(store: &Memory, source: Source, actor: Actor, content: &str) -> EventId {
        let event = Event::message(source, SessionId::new(), actor, content, serde_json::json!({}));
        let id = event.id;
        store.0.lock().unwrap().push(event);

        id
    }

    fn quote(text: &str) -> Cite {
        Cite::Message(text.into())
    }

    fn note_of(node: &Node) -> (&str, &str, &BTreeMap<String, String>) {
        match &node.body {
            NodeBody::Note { kind, title, properties } => (kind, title, properties),
            NodeBody::Citation(_) => panic!("expected a note"),
        }
    }

    fn quoted(node: &Node) -> (EventId, &str) {
        match &node.body {
            NodeBody::Citation(CitationSource::Message { event, quote }) => (*event, quote),
            _ => panic!("expected a message citation"),
        }
    }

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

        let result = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), &[]).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_node_checks_its_kind_and_properties_against_the_schema() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let undeclared = BTreeMap::from([("colour".to_string(), "red".to_string())]);

        let unknown_kind = service.add_node(Source::fixture(), Actor::Human, "concepts", "idea", "A", Default::default(), &[]).await;
        let unknown_property = service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "B", undeclared, &[]).await;
        let subset = service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "C", Default::default(), &[]).await;

        assert!(unknown_kind.unwrap_err().to_string().contains("concept"));
        assert!(unknown_property.unwrap_err().to_string().contains("definition"));
        assert!(subset.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn add_edge_checks_its_kind_and_ends_against_the_schema() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), &[]).await.unwrap();

        let unknown_kind = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "blocks", "Q").await;
        let wrong_from = service.add_edge(Source::fixture(), Actor::Human, "decisions", "Q", "answers", "Q").await;
        let wrong_to = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "O").await;
        let valid = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await;

        assert!(unknown_kind.unwrap_err().to_string().contains("answers"));
        assert!(wrong_from.unwrap_err().to_string().contains("from"));
        assert!(wrong_to.unwrap_err().to_string().contains("to"));
        assert!(valid.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn add_node_fails_when_the_maps_schema_is_gone() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        let gone = AppService::new(&store, NoSchemas);

        let result = gone.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), &[]).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_node_records_its_properties() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let properties = BTreeMap::from([("definition".to_string(), "One log.".to_string())]);

        service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "Event log", properties.clone(), &[]).await.unwrap();

        let map = service.map(&Source::fixture().path, "concepts").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        assert_eq!(note_of(&nodes[0]).2, &properties);
    }

    #[tokio::test]
    async fn blank_kinds_and_titles_are_errors() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();

        let map = service.create_map(Source::fixture(), Actor::Human, " ").await;
        let node = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "", Default::default(), &[]).await;
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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), &[]).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let again = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await;
        let other_kind = service.add_edge(Source::fixture(), Actor::Human, "decisions", "Q", "raises", "O").await;

        assert!(again.is_err());
        assert!(other_kind.is_ok());
    }

    #[tokio::test]
    async fn a_node_title_is_unique_within_its_map_but_not_across_maps() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Why?", Default::default(), &[]).await.unwrap();

        let same_map = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "Why?", Default::default(), &[]).await;
        let other_map = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "Why?", Default::default(), &[]).await;

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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "option", "O", Default::default(), &[]).await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let nodes = service.nodes(map.id).await.unwrap();

        let found: Vec<(&str, &str)> = nodes.iter().map(|n| (note_of(n).0, note_of(n).1)).collect();
        assert_eq!(found, vec![("question", "Q"), ("option", "O")]);
        assert!(service.map(&Source::fixture().path, "nope").await.is_err());
    }

    #[tokio::test]
    async fn an_edge_links_two_titled_nodes_and_a_missing_title_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), &[]).await.unwrap();

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
            service.add_node(Source::fixture(), Actor::Human, map, kind, title, Default::default(), &[]).await.unwrap();
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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), &[]).await.unwrap();
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
        service.add_node(other, Actor::Human, "tasks", "task", "T", Default::default(), &[]).await.unwrap();

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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), &[]).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let detail = service.map_detail(map.id).await.unwrap().unwrap();

        let titles: Vec<&str> = detail.nodes.iter().map(|n| note_of(n).1).collect();
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
    async fn a_quote_records_a_citation_node_and_a_cites_edge_from_the_note() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        let said = say(&store, Source::fixture(), Actor::Human, "Maybe a node is a Node and Citation a subtype");

        service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "Q", Default::default(), &[quote("Citation a subtype")]).await.unwrap();

        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        let edges = service.edges(map.id).await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(note_of(&nodes[0]).1, "Q");
        assert_eq!(quoted(&nodes[1]), (said, "Citation a subtype"));
        let found: Vec<(&str, NodeId, NodeId)> = edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(found, vec![("cites", nodes[0].id, nodes[1].id)]);
    }

    #[tokio::test]
    async fn a_quote_is_found_in_human_messages_first_then_agent_messages_newest_first() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        say(&store, Source::fixture(), Actor::Human, "keep the log, older");
        let newer_human = say(&store, Source::fixture(), Actor::Human, "keep the log, newer");
        say(&store, Source::fixture(), Actor::Agent, "we keep the log");
        let agent_only = say(&store, Source::fixture(), Actor::Agent, "a plan for the schema");
        let newest_agent = say(&store, Source::fixture(), Actor::Agent, "a plan for the schema, again");
        let (log, plan) = ([quote("keep the log")], [quote("a plan for")]);

        service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "A", Default::default(), &log).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "B", Default::default(), &plan).await.unwrap();

        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        assert_eq!(quoted(&nodes[1]).0, newer_human);
        assert_eq!(quoted(&nodes[3]).0, newest_agent);
        assert_ne!(quoted(&nodes[3]).0, agent_only);
    }

    #[tokio::test]
    async fn a_quote_found_in_no_message_of_the_project_is_an_error_and_records_nothing() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        let other = Source { client: "claude".into(), path: "/other".into() };
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        say(&store, Source::fixture(), Actor::Human, "found");
        say(&store, other, Actor::Human, "elsewhere");
        let quotes = [quote("found"), quote("elsewhere")];

        let result = service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "Q", Default::default(), &quotes).await;

        assert!(result.unwrap_err().to_string().contains("verbatim"));
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_blank_quote_is_an_error_and_records_nothing() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        say(&store, Source::fixture(), Actor::Human, "hello there");

        let result = service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "Q", Default::default(), &[quote(" ")]).await;

        assert!(result.unwrap_err().to_string().contains("quote"));
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_citation_with_an_equal_source_is_reused_by_a_later_note_and_within_one_add() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        say(&store, Source::fixture(), Actor::Human, "one two three");
        let (one, two) = ([quote("one two"), quote("one two")], [quote("one two"), quote("two three")]);

        service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "A", Default::default(), &one).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "B", Default::default(), &two).await.unwrap();

        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        let edges = service.edges(map.id).await.unwrap();
        let citations: Vec<&str> = nodes.iter().filter(|n| matches!(n.body, NodeBody::Citation(_))).map(|n| quoted(n).1).collect();
        assert_eq!(citations, vec!["one two", "two three"]);
        let (a, b) = (nodes[0].id, nodes[2].id);
        let (shared, second) = (nodes[1].id, nodes[3].id);
        let found: Vec<(NodeId, NodeId)> = edges.iter().map(|e| (e.from, e.to)).collect();
        assert_eq!(found, vec![(a, shared), (b, shared), (b, second)]);
    }

    #[tokio::test]
    async fn a_taken_title_with_a_quote_records_no_citation() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        say(&store, Source::fixture(), Actor::Human, "hello");

        let result = service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "Q", Default::default(), &[quote("hello")]).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_cites_edge_cannot_be_added_between_notes() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), &[]).await.unwrap();

        let result = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "cites", "Q").await;

        assert!(result.unwrap_err().to_string().contains("citation"));
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn map_summaries_count_notes_and_edges_other_than_cites() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        say(&store, Source::fixture(), Actor::Human, "hello");
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), &[quote("hello")]).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), &[]).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let summaries = service.map_summaries(&Source::fixture().path).await.unwrap();

        let found: Vec<(usize, usize)> = summaries.iter().map(|s| (s.nodes, s.edges)).collect();
        assert_eq!(found, vec![(2, 1)]);
    }

    fn excerpted(node: &Node) -> (&Path, (u32, u32), &str) {
        match &node.body {
            NodeBody::Citation(CitationSource::File { path, lines, excerpt }) => (path, *lines, excerpt),
            _ => panic!("expected a file citation"),
        }
    }

    fn file(path: &str, from: u32, to: u32) -> Cite {
        Cite::File { path: path.into(), lines: (from, to) }
    }

    /// A project in a temp dir with a `decisions` map and the file `a.rs` of five lines.
    async fn project() -> (TempDir, Source, Memory) {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("a.rs"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        let source = Source { client: "claude".into(), path: dir.0.clone() };
        let store = Memory::default();
        AppService::new(&store, Schemas).create_map(source.clone(), Actor::Human, "decisions").await.unwrap();

        (dir, source, store)
    }

    async fn add(store: &Memory, source: &Source, title: &str, cites: &[Cite]) -> Result<()> {
        AppService::new(store, Schemas).add_node(source.clone(), Actor::Agent, "decisions", "question", title, Default::default(), cites).await
    }

    #[tokio::test]
    async fn a_file_cite_records_the_inclusive_lines_the_path_and_a_cites_edge() {
        let (_dir, source, store) = project().await;

        add(&store, &source, "Q", &[file("a.rs", 2, 4)]).await.unwrap();

        let service = AppService::new(&store, Schemas);
        let map = service.map(&source.path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        let edges = service.edges(map.id).await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(excerpted(&nodes[1]), (Path::new("a.rs"), (2, 4), "two\nthree\nfour"));
        let found: Vec<(&str, NodeId, NodeId)> = edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(found, vec![("cites", nodes[0].id, nodes[1].id)]);
    }

    #[tokio::test]
    async fn the_last_line_can_be_cited_and_a_single_line_is_a_range() {
        let (_dir, source, store) = project().await;

        add(&store, &source, "Q", &[file("a.rs", 5, 5), file("a.rs", 1, 1)]).await.unwrap();

        let service = AppService::new(&store, Schemas);
        let map = service.map(&source.path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        assert_eq!(excerpted(&nodes[1]).2, "five");
        assert_eq!(excerpted(&nodes[2]).2, "one");
    }

    #[tokio::test]
    async fn a_note_can_cite_a_message_and_a_file_in_the_order_given() {
        let (_dir, source, store) = project().await;
        let said = say(&store, source.clone(), Actor::Human, "hello there");

        add(&store, &source, "Q", &[file("a.rs", 1, 2), quote("hello")]).await.unwrap();

        let service = AppService::new(&store, Schemas);
        let map = service.map(&source.path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        let edges = service.edges(map.id).await.unwrap();
        assert_eq!(excerpted(&nodes[1]).2, "one\ntwo");
        assert_eq!(quoted(&nodes[2]), (said, "hello"));
        let found: Vec<NodeId> = edges.iter().map(|e| e.to).collect();
        assert_eq!(found, vec![nodes[1].id, nodes[2].id]);
    }

    #[tokio::test]
    async fn bad_line_ranges_are_errors_and_record_nothing() {
        let (_dir, source, store) = project().await;

        let zero = add(&store, &source, "A", &[file("a.rs", 0, 2)]).await;
        let reversed = add(&store, &source, "B", &[file("a.rs", 3, 2)]).await;
        let beyond = add(&store, &source, "C", &[file("a.rs", 4, 6)]).await;

        assert!(zero.is_err());
        assert!(reversed.is_err());
        assert!(beyond.unwrap_err().to_string().contains("5 lines"));
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_missing_file_is_an_error_and_records_nothing() {
        let (_dir, source, store) = project().await;
        say(&store, source.clone(), Actor::Human, "hello");

        let result = add(&store, &source, "Q", &[quote("hello"), file("nope.rs", 1, 1)]).await;

        assert!(result.unwrap_err().to_string().contains("nope.rs"));
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn an_absolute_path_is_an_error_even_inside_the_project() {
        let (dir, source, store) = project().await;

        let result = add(&store, &source, "Q", &[Cite::File { path: dir.0.join("a.rs"), lines: (1, 1) }]).await;

        assert!(result.unwrap_err().to_string().contains("absolute"));
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_path_leaving_the_project_through_dots_is_an_error_but_dots_within_it_resolve() {
        let (dir, source, store) = project().await;
        let outer = TempDir::new();
        std::fs::write(outer.0.join("secret.rs"), "s\n").unwrap();
        std::fs::create_dir(dir.0.join("sub")).unwrap();
        let escape = format!("../{}/secret.rs", outer.0.file_name().unwrap().to_str().unwrap());

        let outside = add(&store, &source, "A", &[file(&escape, 1, 1)]).await;
        let inside = add(&store, &source, "B", &[file("sub/../a.rs", 1, 1)]).await;

        assert!(outside.unwrap_err().to_string().contains("outside"));
        assert!(inside.is_ok());
        let service = AppService::new(&store, Schemas);
        let map = service.map(&source.path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        assert_eq!(excerpted(&nodes[1]).0, Path::new("a.rs"));
    }

    #[tokio::test]
    async fn every_spelling_of_a_path_reuses_one_citation() {
        let (_dir, source, store) = project().await;

        add(&store, &source, "Q", &[file("a.rs", 1, 2), file("./a.rs", 1, 2)]).await.unwrap();

        let service = AppService::new(&store, Schemas);
        let map = service.map(&source.path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(excerpted(&nodes[1]).0, Path::new("a.rs"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_symlink_leading_outside_the_project_is_an_error() {
        let (dir, source, store) = project().await;
        let outer = TempDir::new();
        std::fs::write(outer.0.join("secret.rs"), "s\n").unwrap();
        std::os::unix::fs::symlink(outer.0.join("secret.rs"), dir.0.join("link.rs")).unwrap();

        let result = add(&store, &source, "Q", &[file("link.rs", 1, 1)]).await;

        assert!(result.unwrap_err().to_string().contains("outside"));
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_file_citation_with_an_equal_source_is_reused_until_the_text_changes() {
        let (dir, source, store) = project().await;

        add(&store, &source, "A", &[file("a.rs", 1, 2), file("a.rs", 1, 2)]).await.unwrap();
        add(&store, &source, "B", &[file("a.rs", 1, 2)]).await.unwrap();
        std::fs::write(dir.0.join("a.rs"), "uno\ntwo\n").unwrap();
        add(&store, &source, "C", &[file("a.rs", 1, 2)]).await.unwrap();

        let service = AppService::new(&store, Schemas);
        let map = service.map(&source.path, "decisions").await.unwrap();
        let nodes = service.nodes(map.id).await.unwrap();
        let edges = service.edges(map.id).await.unwrap();
        let excerpts: Vec<&str> = nodes.iter().filter(|n| matches!(n.body, NodeBody::Citation(_))).map(|n| excerpted(n).2).collect();
        assert_eq!(excerpts, vec!["one\ntwo", "uno\ntwo"]);
        let (a, b, c) = (nodes[0].id, nodes[2].id, nodes[3].id);
        let (old, new) = (nodes[1].id, nodes[4].id);
        let found: Vec<(NodeId, NodeId)> = edges.iter().map(|e| (e.from, e.to)).collect();
        assert_eq!(found, vec![(a, old), (b, old), (c, new)]);
    }

    #[tokio::test]
    async fn create_map_passes_store_errors_through() {
        let result = AppService::new(Broken, Schemas).create_map(Source::fixture(), Actor::Human, "decisions").await;

        assert!(result.is_err());
    }

}
