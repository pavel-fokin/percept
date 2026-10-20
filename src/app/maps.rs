use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::AppService;
use crate::core::{Actor, Event, EventId, EventStore, Map, MapId, Node, NodeId, Order, Payload, Policy, Reference, SchemaStore, SessionId, Source};
use crate::shared::Result;

#[derive(Debug, PartialEq)]
pub enum Ref {
    Message(String),
    File { path: PathBuf, lines: (u32, u32) },
}

/// A map with its nodes newest first and its edges in log order.
/// `order` is how its schema says to list them.
/// `messages` holds every message its nodes cite.
pub struct MapDetail {
    pub map: Map,
    pub order: Order,
    pub messages: BTreeMap<EventId, CitedMessage>,
}

/// Where a cited message sits: its session and when it was said.
#[derive(Clone, Copy)]
pub struct CitedMessage {
    pub session: SessionId,
    pub created_at: jiff::Timestamp,
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

    /// The project's maps in log order, each with its nodes and edges, from one read of the log.
    pub async fn maps(&self, project: &Path) -> Result<Vec<Map>> {
        let mut maps = self.all_maps().await?;
        maps.retain(|m| m.path == project);

        Ok(maps)
    }

    /// Every map across all projects in log order, each with its nodes and edges, from one read of the log.
    pub async fn all_maps(&self) -> Result<Vec<Map>> {
        Ok(Map::all(&self.store.all().await?))
    }

    /// The project's map of the given kind with its nodes and edges, or an error if it has none.
    pub async fn map(&self, project: &Path, kind: &str) -> Result<Map> {
        find_map(&self.store.all().await?, project, kind)
    }

    /// The map with the given id, its nodes newest first and its edges in log order, from one read of the log.
    pub async fn map_detail(&self, id: MapId) -> Result<Option<MapDetail>> {
        let events = self.store.all().await?;

        let Some(mut map) = Map::all(&events).into_iter().find(|m| m.id == id) else {
            return Ok(None);
        };
        map.nodes.reverse();

        let order = self.schemas.schema(&map.path, &map.kind).await?.order();

        let cited: BTreeSet<EventId> = map
            .nodes
            .iter()
            .flat_map(|n| &n.references)
            .filter_map(|r| match r {
                Reference::Message { event, .. } => Some(*event),
                Reference::File { .. } => None,
            })
            .collect();
        let messages = events
            .iter()
            .filter(|e| cited.contains(&e.id))
            .filter_map(|e| match &e.payload {
                Payload::Message { session, .. } => Some((e.id, CitedMessage { session: *session, created_at: e.created_at })),
                _ => None,
            })
            .collect();

        Ok(Some(MapDetail { map, order, messages }))
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

        let (_, map) = self.project_map(&source.path, map_kind).await?;

        let (from, to) = (find_node(&map.nodes, map_kind, from)?.id, find_node(&map.nodes, map_kind, to)?.id);

        self.write(&map, Event::edge_added(source, actor, map.id, kind, from, to)).await
    }

    /// Records the removal of the edge of this kind between the nodes with these titles on the project's map of the given kind.
    pub async fn remove_edge(
        &self,
        source: Source,
        actor: Actor,
        map_kind: &str,
        from: &str,
        kind: &str,
        to: &str,
    ) -> Result<()> {
        // Held until return, so the edge cannot change between the check and the append.
        let _lock = self.store.lock().await?;

        let (_, map) = self.project_map(&source.path, map_kind).await?;

        let (from, to) = (find_node(&map.nodes, map_kind, from)?.id, find_node(&map.nodes, map_kind, to)?.id);

        self.write(&map, Event::edge_removed(source, actor, map.id, kind, from, to)).await
    }

    /// Records the removal of the node with this title, and with it every edge from or to it, from the project's map of the given kind.
    pub async fn remove_node(&self, source: Source, actor: Actor, map_kind: &str, title: &str) -> Result<()> {
        // Held until return, so the node cannot change between the check and the append.
        let _lock = self.store.lock().await?;

        let (_, map) = self.project_map(&source.path, map_kind).await?;
        let node = find_node(&map.nodes, map_kind, title)?.id;

        self.write(&map, Event::node_removed(source, actor, map.id, node)).await
    }

    /// Records a new node on the project's map of the given kind. A title is unique within its map.
    #[allow(clippy::too_many_arguments, reason = "the inputs for adding a node")]
    pub async fn add_node(
        &self,
        source: Source,
        actor: Actor,
        map_kind: &str,
        kind: &str,
        title: &str,
        properties: BTreeMap<String, String>,
        references: Vec<Ref>,
    ) -> Result<()> {
        not_blank("kind", kind)?;

        // Held until return, so parallel adds cannot both pass the check.
        let _lock = self.store.lock().await?;

        let (events, map) = self.project_map(&source.path, map_kind).await?;

        let mut resolved = Vec::new();
        for reference in references {
            resolved.push(resolve_reference(&events, &source.path, reference).await?);
        }

        self.write(&map, Event::node_added(source, actor, map.id, NodeId::new(), kind, title, properties, resolved)).await
    }

    /// Records the full new state of the node with this title on the project's map of the given kind.
    /// Each property in `properties` is set; the others keep their values.
    /// `references` are resolved and appended to the node's own. A new title is unique within its map.
    #[allow(clippy::too_many_arguments, reason = "the inputs for changing a node")]
    pub async fn change_node(
        &self,
        source: Source,
        actor: Actor,
        map_kind: &str,
        title: &str,
        new_title: Option<&str>,
        properties: BTreeMap<String, String>,
        references: Vec<Ref>,
    ) -> Result<()> {
        // Held until return, so the node cannot change between the check and the append.
        let _lock = self.store.lock().await?;

        let (events, map) = self.project_map(&source.path, map_kind).await?;
        let node = find_node(&map.nodes, map_kind, title)?;

        let new_title = new_title.unwrap_or(&node.title);

        let mut merged = node.properties.clone();
        merged.extend(properties);

        if new_title == node.title && merged == node.properties && references.is_empty() {
            return Err(format!("the change leaves {title:?} as it is").into());
        }

        let mut all_references = node.references.clone();
        for reference in references {
            all_references.push(resolve_reference(&events, &source.path, reference).await?);
        }

        self.write(&map, Event::node_changed(source, actor, map.id, node.id, new_title, merged, all_references)).await
    }

    /// The log and the project's map of the given kind, read under the store lock.
    async fn project_map(&self, project: &Path, map_kind: &str) -> Result<(Vec<Event>, Map)> {
        let events = self.store.all().await?;
        let map = find_map(&events, project, map_kind)?;

        Ok((events, map))
    }

    /// The one way a node or an edge event reaches the log: the map, under its schema, and then the policy see it first.
    async fn write(&self, map: &Map, event: Event) -> Result<()> {
        let schema = self.schemas.schema(&map.path, &map.kind).await?;

        map.check(&event, &schema)?;
        Policy::check(&event, map)?;

        self.store.append(&event).await
    }
}

fn find_map(events: &[Event], project: &Path, kind: &str) -> Result<Map> {
    Map::all(events)
        .into_iter()
        .find(|m| m.path == project && m.kind == kind)
        .ok_or_else(|| format!("this project has no {kind} map").into())
}

fn find_node<'a>(nodes: &'a [Node], map_kind: &str, title: &str) -> Result<&'a Node> {
    nodes
        .iter()
        .find(|n| n.title == title)
        .ok_or_else(|| format!("the {map_kind} map has no node titled {title:?}").into())
}

async fn resolve_reference(events: &[Event], project: &Path, reference: Ref) -> Result<Reference> {
    match reference {
        Ref::Message(quote) => {
            not_blank("message reference", &quote)?;

            let event = [Actor::Human, Actor::Agent].into_iter().find_map(|actor| {
                events.iter().rev().find(|event| {
                    event.source.path == project && event.actor == actor
                        && matches!(&event.payload, Payload::Message { content, .. } if content.contains(&quote))
                })
            }).ok_or("no matching message in this project; copy the quote verbatim from a recorded message")?;

            Ok(Reference::Message { event: event.id, quote })
        }
        Ref::File { path, lines: (from, to) } => {
            if path.as_os_str().is_empty() || path.is_absolute() {
                return Err("a file reference path must be nonempty and relative to the project".into());
            }
            if from == 0 || from > to {
                return Err("a file reference range must use 1-based lines with from <= to".into());
            }

            let root = tokio::fs::canonicalize(project).await
                .map_err(|error| format!("cannot read project {}: {error}", project.display()))?;
            let target = tokio::fs::canonicalize(root.join(&path)).await
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let relative = target.strip_prefix(&root).map_err(|_| "a file reference must stay inside the project")?.to_path_buf();
            let text = tokio::fs::read_to_string(&target).await
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;

            let excerpt: Vec<_> = text.lines().skip((from - 1) as usize).take((to - from + 1) as usize).collect();
            if excerpt.len() != (to - from + 1) as usize {
                return Err(format!("file reference {}:{from}-{to} exceeds the file's lines", path.display()).into());
            }

            Ok(Reference::File { path: relative, lines: (from, to), excerpt: excerpt.join("\n") })
        }
    }
}

fn not_blank(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be blank").into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Memory, NoSchemas, Schemas};

    #[tokio::test]
    async fn the_same_map_kind_can_be_created_in_two_projects_but_not_twice_in_one() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        let other = Source { client: "claude-code".into(), path: "/other".into() };
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
        let other = Source { client: "claude-code".into(), path: "/other".into() };
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

        let result = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), Vec::new()).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_node_checks_its_kind_and_properties_against_the_schema() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let undeclared = BTreeMap::from([("colour".to_string(), "red".to_string())]);

        let unknown_kind = service.add_node(Source::fixture(), Actor::Human, "concepts", "idea", "A", Default::default(), Vec::new()).await;
        let unknown_property = service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "B", undeclared, Vec::new()).await;
        let subset = service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "C", Default::default(), Vec::new()).await;

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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();

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

        let result = gone.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), Vec::new()).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_node_records_its_properties() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let properties = BTreeMap::from([("definition".to_string(), "One log.".to_string())]);

        service.add_node(Source::fixture(), Actor::Human, "concepts", "concept", "Event log", properties.clone(), Vec::new()).await.unwrap();

        let nodes = service.map(&Source::fixture().path, "concepts").await.unwrap().nodes;
        assert_eq!(nodes[0].properties, properties);
    }

    #[tokio::test]
    async fn message_references_prefer_the_latest_human_in_the_project() {
        use crate::core::SessionId;
        use serde_json::json;

        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let session = SessionId::new();
        let human = Event::message(Source::fixture(), session, Actor::Human, "Keep the text: verbatim.", json!({}));
        let mut foreign = Source::fixture();
        foreign.path = "/other".into();
        let events = [
            Event::message(Source::fixture(), session, Actor::Human, "Keep the text: verbatim.", json!({})),
            human.clone(),
            Event::message(Source::fixture(), session, Actor::Agent, "Keep the text: verbatim.", json!({})),
            Event::message(foreign, session, Actor::Human, "Keep the text: verbatim.", json!({})),
            Event::message(Source::fixture(), session, Actor::System, "Keep the text: verbatim.", json!({})),
        ];
        store.0.lock().unwrap().extend(events);

        service.add_node(
            Source::fixture(), Actor::Agent, "concepts", "concept", "Snapshots", BTreeMap::new(),
            vec![Ref::Message("text: verbatim".into()), Ref::Message("Keep the text".into()), Ref::Message("text: verbatim".into())],
        ).await.unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events.len(), 7);
        let Payload::NodeAdded { references, .. } = &events.last().unwrap().payload else { panic!("expected node.added") };
        assert_eq!(references, &vec![
            Reference::Message { event: human.id, quote: "text: verbatim".into() },
            Reference::Message { event: human.id, quote: "Keep the text".into() },
            Reference::Message { event: human.id, quote: "text: verbatim".into() },
        ]);
    }

    #[tokio::test]
    async fn message_references_fall_back_to_the_latest_agent() {
        use crate::core::SessionId;
        use serde_json::json;

        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let session = SessionId::new();
        let latest = Event::message(Source::fixture(), session, Actor::Agent, "A --flag: exact text Z", json!({}));
        store.0.lock().unwrap().extend([
            Event::message(Source::fixture(), session, Actor::Agent, "A --flag: exact text Z", json!({})),
            latest.clone(),
            Event::message(Source::fixture(), session, Actor::Human, "Unrelated words", json!({})),
        ]);

        service.add_node(
            Source::fixture(), Actor::Human, "concepts", "concept", "Flags", BTreeMap::new(),
            vec![Ref::Message(" --flag: exact text ".into())],
        ).await.unwrap();

        let events = store.0.lock().unwrap();
        let Payload::NodeAdded { references, .. } = &events.last().unwrap().payload else { panic!("expected node.added") };
        assert_eq!(references, &vec![Reference::Message { event: latest.id, quote: " --flag: exact text ".into() }]);
    }

    #[tokio::test]
    async fn a_bad_message_reference_writes_no_node_or_partial_references() {
        use crate::core::SessionId;
        use serde_json::json;

        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        store.0.lock().unwrap().push(Event::message(
            Source::fixture(), SessionId::new(), Actor::Human, "Exact text", json!({}),
        ));

        for quote in ["", " \n ", "exact text", "Exact  text", "missing"] {
            let result = service.add_node(
                Source::fixture(), Actor::Human, "concepts", "concept", "Snapshots", BTreeMap::new(),
                vec![Ref::Message("Exact text".into()), Ref::Message(quote.into())],
            ).await;

            let error = result.unwrap_err().to_string();
            assert!(error.contains(if quote.trim().is_empty() { "blank" } else { "copy the quote verbatim" }));
            assert_eq!(store.0.lock().unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn system_messages_other_projects_and_non_messages_are_not_reference_sources() {
        use crate::core::SessionId;
        use serde_json::json;

        let session = SessionId::new();
        let project = Source::fixture().path;
        let events = [
            Event::message(Source::fixture(), session, Actor::System, "Match", json!({})),
            Event::message(Source { client: "claude-code".into(), path: "/other".into() }, session, Actor::Human, "Match", json!({})),
            Event::subagent_reported(Source::fixture(), session, crate::core::ClientAgentId::new("a".into()), "Match", json!({})),
        ];

        let result = resolve_reference(&events, &project, Ref::Message("Match".into())).await;

        assert!(result.is_err());
    }

    async fn file_project() -> Source {
        let path = std::env::temp_dir().join(format!("percept-references-{}", uuid::Uuid::now_v7()));
        tokio::fs::create_dir_all(&path).await.unwrap();

        Source { client: "percept-cli".into(), path }
    }

    #[tokio::test]
    async fn file_references_save_inclusive_single_and_last_lines() {
        let source = file_project().await;
        tokio::fs::write(source.path.join("text.txt"), "first\r\n\nthird\nlast").await.unwrap();
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(source.clone(), Actor::Human, "concepts").await.unwrap();

        service.add_node(
            source.clone(), Actor::Human, "concepts", "concept", "Excerpts", BTreeMap::new(),
            vec![
                Ref::File { path: "text.txt".into(), lines: (1, 1) },
                Ref::File { path: "text.txt".into(), lines: (2, 3) },
                Ref::File { path: "text.txt".into(), lines: (4, 4) },
            ],
        ).await.unwrap();
        tokio::fs::write(source.path.join("text.txt"), "changed").await.unwrap();

        let nodes = service.map(&source.path, "concepts").await.unwrap().nodes;
        assert_eq!(nodes[0].references, vec![
            Reference::File { path: "text.txt".into(), lines: (1, 1), excerpt: "first".into() },
            Reference::File { path: "text.txt".into(), lines: (2, 3), excerpt: "\nthird".into() },
            Reference::File { path: "text.txt".into(), lines: (4, 4), excerpt: "last".into() },
        ]);
        assert_eq!(store.0.lock().unwrap().len(), 2);
        tokio::fs::remove_dir_all(&source.path).await.unwrap();
    }

    #[tokio::test]
    async fn mixed_references_keep_order_multiplicity_and_canonical_paths() {
        use crate::core::SessionId;
        use serde_json::json;

        let source = file_project().await;
        tokio::fs::create_dir(source.path.join("src")).await.unwrap();
        tokio::fs::write(source.path.join("src/a:b.rs"), "code\n").await.unwrap();
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(source.clone(), Actor::Human, "concepts").await.unwrap();
        let message = Event::message(source.clone(), SessionId::new(), Actor::Human, "Keep code", json!({}));
        store.0.lock().unwrap().push(message.clone());

        service.add_node(
            source.clone(), Actor::Human, "concepts", "concept", "Code", BTreeMap::new(),
            vec![
                Ref::File { path: "./src/../src/a:b.rs".into(), lines: (1, 1) },
                Ref::Message("Keep code".into()),
                Ref::File { path: "src/a:b.rs".into(), lines: (1, 1) },
            ],
        ).await.unwrap();

        let events = store.0.lock().unwrap().clone();
        let Payload::NodeAdded { references, .. } = &events.last().unwrap().payload else { panic!("expected node.added") };
        assert_eq!(references, &vec![
            Reference::File { path: "src/a:b.rs".into(), lines: (1, 1), excerpt: "code".into() },
            Reference::Message { event: message.id, quote: "Keep code".into() },
            Reference::File { path: "src/a:b.rs".into(), lines: (1, 1), excerpt: "code".into() },
        ]);
        assert_eq!(events.len(), 3);
        tokio::fs::remove_dir_all(&source.path).await.unwrap();
    }

    #[tokio::test]
    async fn invalid_file_references_leave_the_log_unchanged() {
        let source = file_project().await;
        tokio::fs::write(source.path.join("text.txt"), "first\nsecond\n").await.unwrap();
        tokio::fs::write(source.path.join("empty.txt"), "").await.unwrap();
        tokio::fs::write(source.path.join("binary"), [0xff]).await.unwrap();
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(source.clone(), Actor::Human, "concepts").await.unwrap();
        let invalid = [
            Ref::File { path: "".into(), lines: (1, 1) },
            Ref::File { path: source.path.join("text.txt"), lines: (1, 1) },
            Ref::File { path: "missing".into(), lines: (1, 1) },
            Ref::File { path: ".".into(), lines: (1, 1) },
            Ref::File { path: "empty.txt".into(), lines: (1, 1) },
            Ref::File { path: "binary".into(), lines: (1, 1) },
            Ref::File { path: "text.txt".into(), lines: (0, 1) },
            Ref::File { path: "text.txt".into(), lines: (2, 1) },
            Ref::File { path: "text.txt".into(), lines: (1, 3) },
            Ref::File { path: "text.txt".into(), lines: (3, 3) },
        ];

        for reference in invalid {
            let result = service.add_node(
                source.clone(), Actor::Human, "concepts", "concept", "Excerpts", BTreeMap::new(),
                vec![Ref::File { path: "text.txt".into(), lines: (1, 1) }, reference],
            ).await;

            assert!(result.is_err());
            assert_eq!(store.0.lock().unwrap().len(), 1);
        }
        tokio::fs::remove_dir_all(&source.path).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn file_references_resolve_internal_symlinks_and_reject_escapes() {
        let source = file_project().await;
        let project = source.path.join("project");
        tokio::fs::create_dir(&project).await.unwrap();
        tokio::fs::write(project.join("inside.txt"), "inside").await.unwrap();
        tokio::fs::write(source.path.join("outside.txt"), "outside").await.unwrap();
        tokio::fs::symlink("inside.txt", project.join("alias.txt")).await.unwrap();
        tokio::fs::symlink("../outside.txt", project.join("escape.txt")).await.unwrap();

        let inside = resolve_reference(&[], &project, Ref::File { path: "alias.txt".into(), lines: (1, 1) }).await.unwrap();
        let escape = resolve_reference(&[], &project, Ref::File { path: "escape.txt".into(), lines: (1, 1) }).await;
        let parent = resolve_reference(&[], &project, Ref::File { path: "../outside.txt".into(), lines: (1, 1) }).await;

        assert_eq!(inside, Reference::File { path: "inside.txt".into(), lines: (1, 1), excerpt: "inside".into() });
        assert!(escape.is_err());
        assert!(parent.is_err());
        tokio::fs::remove_dir_all(&source.path).await.unwrap();
    }

    #[tokio::test]
    async fn blank_kinds_and_titles_are_errors() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();

        let map = service.create_map(Source::fixture(), Actor::Human, " ").await;
        let node = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "", Default::default(), Vec::new()).await;
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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();
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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Why?", Default::default(), Vec::new()).await.unwrap();

        let same_map = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "Why?", Default::default(), Vec::new()).await;
        let other_map = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "Why?", Default::default(), Vec::new()).await;

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
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();

        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let found: Vec<(&str, &str)> = map.nodes.iter().map(|n| (n.kind.as_str(), n.title.as_str())).collect();
        assert_eq!(found, vec![("question", "Q"), ("option", "O")]);
        assert!(service.map(&Source::fixture().path, "nope").await.is_err());
    }

    #[tokio::test]
    async fn an_edge_links_two_titled_nodes_and_a_missing_title_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();

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
            service.add_node(Source::fixture(), Actor::Human, map, kind, title, Default::default(), Vec::new()).await.unwrap();
        }
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "tasks", "T", "blocks", "U").await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "Q", "raises", "O").await.unwrap();

        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let found: Vec<(&str, NodeId, NodeId)> = map.edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        let nodes = &map.nodes;
        assert_eq!(found, vec![("answers", nodes[1].id, nodes[0].id), ("raises", nodes[0].id, nodes[1].id)]);
    }

    #[tokio::test]
    async fn maps_carry_each_maps_nodes_and_edges() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let maps = service.maps(&Source::fixture().path).await.unwrap();

        let found: Vec<(&str, usize, usize)> = maps.iter().map(|m| (m.kind.as_str(), m.nodes.len(), m.edges.len())).collect();
        assert_eq!(found, vec![("decisions", 2, 1), ("tasks", 1, 0)]);
    }

    #[tokio::test]
    async fn all_maps_span_projects_and_carry_each_maps_path() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        let other = Source { client: "claude-code".into(), path: "/other".into() };
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(other.clone(), Actor::Human, "tasks").await.unwrap();
        service.add_node(other, Actor::Human, "tasks", "task", "T", Default::default(), Vec::new()).await.unwrap();

        let maps = service.all_maps().await.unwrap();

        let found: Vec<(&str, &Path, usize)> = maps.iter().map(|m| (m.kind.as_str(), m.path.as_path(), m.nodes.len())).collect();
        assert_eq!(found, vec![("decisions", Source::fixture().path.as_path(), 0), ("tasks", Path::new("/other"), 1)]);
    }

    #[tokio::test]
    async fn map_detail_has_nodes_newest_first_and_edges_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let detail = service.map_detail(map.id).await.unwrap().unwrap();

        let titles: Vec<&str> = detail.map.nodes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["O", "Q"]);
        let edges: Vec<(&str, NodeId, NodeId)> = detail.map.edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(edges, vec![("answers", detail.map.nodes[0].id, detail.map.nodes[1].id)]);
        assert_eq!(detail.map.kind, "decisions");
    }

    #[tokio::test]
    async fn map_detail_locates_the_messages_its_nodes_cite() {
        use crate::core::SessionId;
        use serde_json::json;

        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "concepts").await.unwrap();
        let session = SessionId::new();
        let cited = Event::message(Source::fixture(), session, Actor::Human, "Keep the text", json!({}));
        let other = Event::message(Source::fixture(), session, Actor::Human, "Unrelated", json!({}));
        store.0.lock().unwrap().extend([cited.clone(), other]);
        service.add_node(
            Source::fixture(), Actor::Human, "concepts", "concept", "C", BTreeMap::new(),
            vec![Ref::Message("Keep the text".into())],
        ).await.unwrap();
        let map = service.map(&Source::fixture().path, "concepts").await.unwrap();

        let detail = service.map_detail(map.id).await.unwrap().unwrap();

        assert_eq!(detail.messages.len(), 1);
        assert_eq!(detail.messages[&cited.id].session, session);
        assert_eq!(detail.messages[&cited.id].created_at, cited.created_at);
    }

    #[tokio::test]
    async fn map_detail_carries_the_order_its_schema_declares() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "levels").await.unwrap();
        let decisions = service.map(&Source::fixture().path, "decisions").await.unwrap();
        let levels = service.map(&Source::fixture().path, "levels").await.unwrap();

        let decisions = service.map_detail(decisions.id).await.unwrap().unwrap();
        let levels = service.map_detail(levels.id).await.unwrap().unwrap();

        assert_eq!(decisions.order, Order::Added);
        assert_eq!(levels.order, Order::Edges);
    }

    #[tokio::test]
    async fn map_detail_fails_when_the_maps_schema_is_gone() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();
        let gone = AppService::new(&store, NoSchemas);

        let result = gone.map_detail(map.id).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn map_detail_of_an_unknown_id_is_none() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();

        let detail = service.map_detail(MapId::new()).await.unwrap();

        assert!(detail.is_none());
    }

    fn props(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    async fn glossary(store: &Memory) -> AppService<&Memory, Schemas> {
        let service = AppService::new(store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "glossary").await.unwrap();
        service.add_node(
            Source::fixture(), Actor::Human, "glossary", "term", "Log",
            props(&[("definition", "One log."), ("source", "docs")]), Vec::new(),
        ).await.unwrap();

        service
    }

    async fn glossary_nodes(service: &AppService<&Memory, Schemas>) -> Vec<Node> {
        service.map(&Source::fixture().path, "glossary").await.unwrap().nodes
    }

    #[tokio::test]
    async fn a_change_writes_the_full_new_state_and_keeps_unnamed_properties() {
        let store = Memory::default();
        let service = glossary(&store).await;
        let map = service.map(&Source::fixture().path, "glossary").await.unwrap();
        let before = glossary_nodes(&service).await;

        service.change_node(
            Source::fixture(), Actor::Human, "glossary", "Log", Some("Event log"),
            props(&[("definition", "Append-only.")]), Vec::new(),
        ).await.unwrap();

        let last = store.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.actor, Actor::Human);
        assert_eq!(last.payload, Payload::NodeChanged {
            map: map.id,
            node: before[0].id,
            title: "Event log".into(),
            properties: props(&[("definition", "Append-only."), ("source", "docs")]),
            references: Vec::new(),
        });
        let after = glossary_nodes(&service).await;
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].title, "Event log");
        assert_eq!(after[0].changes.len(), 2);
    }

    #[tokio::test]
    async fn two_positionals_keep_the_title() {
        let store = Memory::default();
        let service = glossary(&store).await;

        service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, props(&[("source", "spec")]), Vec::new()).await.unwrap();

        let nodes = glossary_nodes(&service).await;
        assert_eq!(nodes[0].title, "Log");
        assert_eq!(nodes[0].properties, props(&[("definition", "One log."), ("source", "spec")]));
    }

    #[tokio::test]
    async fn an_empty_value_is_set_like_any_other() {
        let store = Memory::default();
        let service = glossary(&store).await;

        service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, props(&[("source", "")]), Vec::new()).await.unwrap();

        let nodes = glossary_nodes(&service).await;
        assert_eq!(nodes[0].properties, props(&[("definition", "One log."), ("source", "")]));
    }

    #[tokio::test]
    async fn a_reference_is_appended_to_the_existing_ones() {
        use crate::core::SessionId;
        use serde_json::json;

        let store = Memory::default();
        let service = glossary(&store).await;
        let first = Event::message(Source::fixture(), SessionId::new(), Actor::Human, "First quote", json!({}));
        let second = Event::message(Source::fixture(), SessionId::new(), Actor::Human, "Second quote", json!({}));
        store.0.lock().unwrap().extend([first.clone(), second.clone()]);

        service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, BTreeMap::new(), vec![Ref::Message("First quote".into())]).await.unwrap();
        service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, BTreeMap::new(), vec![Ref::Message("Second quote".into())]).await.unwrap();

        let nodes = glossary_nodes(&service).await;
        assert_eq!(nodes[0].references, vec![
            Reference::Message { event: first.id, quote: "First quote".into() },
            Reference::Message { event: second.id, quote: "Second quote".into() },
        ]);
    }

    #[tokio::test]
    async fn a_rename_is_not_blank_and_unique_within_its_map_but_may_keep_the_old_title() {
        let store = Memory::default();
        let service = glossary(&store).await;
        service.add_node(Source::fixture(), Actor::Human, "glossary", "term", "Other", BTreeMap::new(), Vec::new()).await.unwrap();

        let taken = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", Some("Other"), BTreeMap::new(), Vec::new()).await;
        let blank = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", Some(" "), BTreeMap::new(), Vec::new()).await;
        let same = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", Some("Log"), props(&[("source", "spec")]), Vec::new()).await;

        assert!(taken.unwrap_err().to_string().contains("\"Other\""));
        assert!(blank.unwrap_err().to_string().contains("blank"));
        assert!(same.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn the_merged_properties_are_checked_against_the_schema() {
        let store = Memory::default();
        let service = glossary(&store).await;

        let result = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, props(&[("colour", "red")]), Vec::new()).await;

        assert!(result.unwrap_err().to_string().contains("colour"));
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_change_that_leaves_the_node_as_it_is_is_an_error() {
        let store = Memory::default();
        let service = glossary(&store).await;

        let nothing = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, BTreeMap::new(), Vec::new()).await;
        let same_title = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", Some("Log"), BTreeMap::new(), Vec::new()).await;
        let same_value = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, props(&[("source", "docs")]), Vec::new()).await;
        let absent_drop = service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, props(&[("definition", "One log."), ("source", "docs")]), Vec::new()).await;

        assert!(nothing.is_err());
        assert!(same_title.is_err());
        assert!(same_value.is_err());
        assert!(absent_drop.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn changing_a_missing_node_or_map_is_an_error() {
        let store = Memory::default();
        let service = glossary(&store).await;

        let node = service.change_node(Source::fixture(), Actor::Human, "glossary", "Nope", None, props(&[("source", "x")]), Vec::new()).await;
        let map = service.change_node(Source::fixture(), Actor::Human, "tasks", "Log", None, props(&[("source", "x")]), Vec::new()).await;

        assert!(node.unwrap_err().to_string().contains("\"Nope\""));
        assert!(map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_changing_a_human_node_and_the_log_stays() {
        let store = Memory::default();
        let service = glossary(&store).await;

        let result = service.change_node(Source::fixture(), Actor::Agent, "glossary", "Log", None, props(&[("source", "x")]), Vec::new()).await;

        assert!(result.unwrap_err().to_string().contains("only a human may change it"));
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn an_agent_may_change_its_own_node_until_a_human_changes_it() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "glossary").await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "glossary", "term", "Log", BTreeMap::new(), Vec::new()).await.unwrap();

        let agent = service.change_node(Source::fixture(), Actor::Agent, "glossary", "Log", None, props(&[("source", "a")]), Vec::new()).await;
        service.change_node(Source::fixture(), Actor::Human, "glossary", "Log", None, props(&[("source", "h")]), Vec::new()).await.unwrap();
        let after_human = service.change_node(Source::fixture(), Actor::Agent, "glossary", "Log", None, props(&[("source", "b")]), Vec::new()).await;

        assert!(agent.is_ok());
        assert!(after_human.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    async fn decisions(store: &Memory, actor: Actor) -> AppService<&Memory, Schemas> {
        let service = AppService::new(store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), actor, "decisions", "O", "answers", "Q").await.unwrap();

        service
    }

    #[tokio::test]
    async fn removing_an_edge_drops_it_from_the_edges_and_the_summary() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Human).await;

        service.remove_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let last = store.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.actor, Actor::Human);
        assert_eq!(last.payload.kind(), crate::core::Kind::EdgeRemoved);
        let after = service.map(&Source::fixture().path, "decisions").await.unwrap();
        assert_eq!((after.nodes.len(), after.edges.len()), (2, 0));
    }

    #[tokio::test]
    async fn removing_a_missing_node_or_edge_is_an_error_and_the_log_stays() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Human).await;

        let from = service.remove_edge(Source::fixture(), Actor::Human, "decisions", "Nope", "answers", "Q").await;
        let to = service.remove_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Nope").await;
        let reversed = service.remove_edge(Source::fixture(), Actor::Human, "decisions", "Q", "answers", "O").await;
        let kind = service.remove_edge(Source::fixture(), Actor::Human, "decisions", "O", "raises", "Q").await;
        let map = service.remove_edge(Source::fixture(), Actor::Human, "tasks", "O", "answers", "Q").await;

        assert!(from.unwrap_err().to_string().contains("\"Nope\""));
        assert!(to.unwrap_err().to_string().contains("\"Nope\""));
        assert!(reversed.unwrap_err().to_string().contains("answers"));
        assert!(kind.unwrap_err().to_string().contains("raises"));
        assert!(map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_removing_a_human_edge_and_the_log_stays() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Human).await;

        let result = service.remove_edge(Source::fixture(), Actor::Agent, "decisions", "O", "answers", "Q").await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("\"O\" answers \"Q\""));
        assert!(error.contains("only a human may remove it"));
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn an_agent_may_remove_its_own_edge_and_a_removed_edge_can_be_added_again() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Agent).await;

        let removed = service.remove_edge(Source::fixture(), Actor::Agent, "decisions", "O", "answers", "Q").await;
        let again = service.add_edge(Source::fixture(), Actor::Agent, "decisions", "O", "answers", "Q").await;

        assert!(removed.is_ok());
        assert!(again.is_ok());
        assert_eq!(service.map(&Source::fixture().path, "decisions").await.unwrap().edges.len(), 1);
        assert_eq!(store.0.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn removing_a_node_drops_it_its_edges_and_the_summary_counts() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Human).await;
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        service.remove_node(Source::fixture(), Actor::Human, "decisions", "Q").await.unwrap();

        let last = store.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.payload.kind(), crate::core::Kind::NodeRemoved);
        let detail = service.map_detail(map.id).await.unwrap().unwrap();
        let titles: Vec<&str> = detail.map.nodes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["O"]);
        assert!(detail.map.edges.is_empty());
        let maps = service.maps(&Source::fixture().path).await.unwrap();
        assert_eq!((maps[0].nodes.len(), maps[0].edges.len()), (1, 0));
    }

    #[tokio::test]
    async fn a_removed_nodes_title_can_be_added_again() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Human).await;
        service.remove_node(Source::fixture(), Actor::Human, "decisions", "Q").await.unwrap();

        let again = service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q", Default::default(), Vec::new()).await;

        assert!(again.is_ok());
        let maps = service.maps(&Source::fixture().path).await.unwrap();
        assert_eq!((maps[0].nodes.len(), maps[0].edges.len()), (2, 0));
    }

    #[tokio::test]
    async fn removing_a_missing_node_or_map_is_an_error_and_the_log_stays() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Human).await;

        let node = service.remove_node(Source::fixture(), Actor::Human, "decisions", "Nope").await;
        let map = service.remove_node(Source::fixture(), Actor::Human, "tasks", "Q").await;

        assert!(node.unwrap_err().to_string().contains("\"Nope\""));
        assert!(map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_removing_a_human_node_and_the_log_stays() {
        let store = Memory::default();
        let service = decisions(&store, Actor::Agent).await;

        let result = service.remove_node(Source::fixture(), Actor::Agent, "decisions", "Q").await;

        assert!(result.unwrap_err().to_string().contains("\"Q\" was written by a human, so only a human may remove it"));
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_removing_its_node_under_a_human_edge_and_the_log_stays() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let result = service.remove_node(Source::fixture(), Actor::Agent, "decisions", "Q").await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("\"O\" answers \"Q\""));
        assert!(error.contains("which removes it"));
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn an_agent_may_remove_its_node_with_agents_edges() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_map(Source::fixture(), Actor::Human, "decisions").await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Agent, "decisions", "O", "answers", "Q").await.unwrap();

        let result = service.remove_node(Source::fixture(), Actor::Agent, "decisions", "Q").await;

        assert!(result.is_ok());
        let maps = service.maps(&Source::fixture().path).await.unwrap();
        assert_eq!((maps[0].nodes.len(), maps[0].edges.len()), (1, 0));
    }

    #[tokio::test]
    async fn create_map_passes_store_errors_through() {
        let result = AppService::new(Broken, Schemas).create_map(Source::fixture(), Actor::Human, "decisions").await;

        assert!(result.is_err());
    }

}
