//! Application layer: `AppService` runs use cases over the domain.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use crate::core::{Actor, ClientSessionId, Edge, Event, EventStore, Map, MapId, Node, NodeId, Payload, Session, SessionId, Source};

mod git;

/// A map with how many nodes and edges it holds.
pub struct MapSummary {
    pub map: Map,
    pub nodes: usize,
    pub edges: usize,
}

pub struct AppService<S: EventStore> {
    store: S,
}

impl<S: EventStore> AppService<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn event_count(&self) -> Result<usize, Box<dyn Error + Send + Sync>> {
        Ok(self.store.all().await?.len())
    }

    /// Sessions newest first.
    pub async fn sessions(&self) -> Result<Vec<Session>, Box<dyn Error + Send + Sync>> {
        let events = self.store.all().await?;

        let mut titles: BTreeMap<SessionId, &str> = BTreeMap::new();
        for e in events.iter().filter(|e| e.actor == Actor::Human) {
            if let Payload::Message { session, content } = &e.payload
                && !content.trim().is_empty()
            {
                titles.entry(*session).or_insert(content);
            }
        }

        let sessions = events
            .iter()
            .rev()
            .filter_map(|e| match &e.payload {
                Payload::SessionCreated { session, client_session_id } => Some(Session {
                    id: *session,
                    client_session_id: client_session_id.clone(),
                    created_at: e.created_at,
                    title: titles.get(session).map(|t| t.to_string()),
                }),
                _ => None,
            })
            .collect();

        Ok(sessions)
    }

    pub async fn session(&self, id: SessionId) -> Result<Option<Session>, Box<dyn Error + Send + Sync>> {
        Ok(self.sessions().await?.into_iter().find(|s| s.id == id))
    }

    /// The session's events in log order, or `None` if the session does not exist.
    pub async fn session_events(&self, id: SessionId) -> Result<Option<Vec<Event>>, Box<dyn Error + Send + Sync>> {
        let events: Vec<Event> = self.store.all().await?.into_iter().filter(|e| e.payload.session() == Some(id)).collect();

        let exists = events.iter().any(|e| matches!(e.payload, Payload::SessionCreated { .. }));
        Ok(exists.then_some(events))
    }

    /// Records a new map. A project holds at most one map of each kind.
    pub async fn create_map(
        &self,
        source: Source,
        actor: Actor,
        kind: &str,
        title: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        not_blank("kind", kind)?;
        not_blank("title", title)?;

        // Held until return, so parallel creates cannot both pass the check.
        let _lock = self.store.lock().await?;

        let taken = self.maps(&source.path).await?.iter().any(|m| m.kind == kind);
        if taken {
            return Err(format!("this project already has a {kind} map").into());
        }

        self.store.append(&Event::map_created(source, actor, MapId::new(), kind, title)).await
    }

    /// The project's maps in log order.
    pub async fn maps(&self, project: &Path) -> Result<Vec<Map>, Box<dyn Error + Send + Sync>> {
        Ok(project_maps(&self.store.all().await?, project))
    }

    /// The project's map of the given kind, or an error if it has none.
    pub async fn map(&self, project: &Path, kind: &str) -> Result<Map, Box<dyn Error + Send + Sync>> {
        let found = self.maps(project).await?.into_iter().find(|m| m.kind == kind);

        found.ok_or_else(|| format!("this project has no {kind} map").into())
    }

    /// The map's nodes in log order.
    pub async fn nodes(&self, map: MapId) -> Result<Vec<Node>, Box<dyn Error + Send + Sync>> {
        let nodes = self
            .store
            .all()
            .await?
            .into_iter()
            .filter_map(|e| match e.payload {
                Payload::NodeAdded { map: id, node, kind, title } if id == map => Some(Node { id: node, kind, title }),
                _ => None,
            })
            .collect();

        Ok(nodes)
    }

    /// The project's maps in log order, each with its node and edge counts, from one read of the log.
    pub async fn map_summaries(&self, project: &Path) -> Result<Vec<MapSummary>, Box<dyn Error + Send + Sync>> {
        let events = self.store.all().await?;

        let mut summaries: Vec<MapSummary> = project_maps(&events, project)
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

    /// The map's edges in log order.
    pub async fn edges(&self, map: MapId) -> Result<Vec<Edge>, Box<dyn Error + Send + Sync>> {
        let edges = self
            .store
            .all()
            .await?
            .into_iter()
            .filter_map(|e| match e.payload {
                Payload::EdgeAdded { map: id, kind, from, to } if id == map => Some(Edge { kind, from, to }),
                _ => None,
            })
            .collect();

        Ok(edges)
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
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
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
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        not_blank("kind", kind)?;
        not_blank("title", title)?;

        // Held until return, so parallel adds cannot both pass the check.
        let _lock = self.store.lock().await?;

        let map = self.map(&source.path, map_kind).await?;
        if self.nodes(map.id).await?.iter().any(|n| n.title == title) {
            return Err(format!("the {map_kind} map already has a node titled {title:?}").into());
        }

        self.store.append(&Event::node_added(source, actor, map.id, NodeId::new(), kind, title)).await
    }

    /// Appends the event to the session the client's session id names, creating the session first if it is new.
    pub async fn record(
        &self,
        client_session_id: ClientSessionId,
        source: Source,
        event: impl FnOnce(Source, SessionId) -> Event,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Held until return, so parallel hooks cannot create the same session twice.
        let _lock = self.store.lock().await?;

        let existing = self.store.session(&client_session_id).await?;
        let session = existing.unwrap_or_else(SessionId::new);

        if existing.is_none() {
            let created = Event::session_created(source.clone(), session, client_session_id);
            self.store.append(&created).await?;
        }

        self.store.append(&event(source, session)).await
    }
}

fn not_blank(name: &str, value: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be blank").into());
    }

    Ok(())
}

/// The project's maps in log order.
fn project_maps(events: &[Event], project: &Path) -> Vec<Map> {
    events
        .iter()
        .filter(|e| e.source.path == project)
        .filter_map(|e| match &e.payload {
            Payload::MapCreated { map, kind, title } => Some(Map { id: *map, kind: kind.clone(), title: title.clone() }),
            _ => None,
        })
        .collect()
}

/// The git repo root containing `dir`, else `dir` itself.
pub async fn source_path(dir: &Path) -> PathBuf {
    git::repo_root(dir).await.unwrap_or_else(|| dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::core::Kind;

    use serde_json::json;

    struct Fixed(usize);

    impl EventStore for Fixed {
        type Lock = ();

        async fn lock(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }

        async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }

        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            Ok((0..self.0)
                .map(|_| Event::session_started(Source::fixture(), SessionId::new(), json!({})))
                .collect())
        }

        async fn append(&self, _: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
    }

    struct Broken;

    impl EventStore for Broken {
        type Lock = ();

        async fn lock(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
            Err("boom".into())
        }

        async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
            Err("boom".into())
        }

        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            Err("boom".into())
        }

        async fn append(&self, _: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            Err("boom".into())
        }
    }

    #[derive(Default)]
    struct Memory(Mutex<Vec<Event>>);

    impl EventStore for &Memory {
        type Lock = ();

        async fn lock(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }

        async fn session(&self, client_session_id: &ClientSessionId) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
            let events = self.0.lock().unwrap();
            Ok(events
                .iter()
                .find_map(|e| match &e.payload {
                    Payload::SessionCreated { session, client_session_id: id } if id == client_session_id => {
                        Some(*session)
                    }
                    _ => None,
                }))
        }

        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            let events = self.0.lock().unwrap();
            Ok(events.clone())
        }

        async fn append(&self, event: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.0.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    fn client_session_id(id: &str) -> ClientSessionId {
        ClientSessionId::new(id.into())
    }

    #[tokio::test]
    async fn counts_the_stored_events() {
        assert_eq!(AppService::new(Fixed(3)).event_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn passes_store_errors_through() {
        assert!(AppService::new(Broken).event_count().await.is_err());
        assert!(AppService::new(Broken).sessions().await.is_err());
        assert!(AppService::new(Broken).session(SessionId::new()).await.is_err());
        assert!(AppService::new(Broken).session_events(SessionId::new()).await.is_err());
    }

    #[tokio::test]
    async fn sessions_are_the_created_events_newest_first() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        let events = store.0.lock().unwrap();
        let found: Vec<(SessionId, &str)> = sessions.iter().map(|s| (s.id, s.client_session_id.as_str())).collect();
        assert_eq!(found, vec![(events[2].payload.session().unwrap(), "b"), (events[0].payload.session().unwrap(), "a")]);
        assert_eq!(sessions[1].created_at, events[0].created_at);
    }

    #[tokio::test]
    async fn session_finds_one_session_by_id() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "hi", json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();
        let id = store.0.lock().unwrap()[0].payload.session().unwrap();

        let found = service.session(id).await.unwrap().unwrap();

        assert_eq!(found.id, id);
        assert_eq!(found.client_session_id.as_str(), "a");
        assert_eq!(found.title.as_deref(), Some("hi"));
        assert!(service.session(SessionId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn session_events_are_the_sessions_own_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "hi", json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Read", json!({}))).await.unwrap();
        let id = store.0.lock().unwrap()[0].payload.session().unwrap();

        let events = service.session_events(id).await.unwrap().unwrap();

        let kinds: Vec<Kind> = events.iter().map(|e| e.payload.kind()).collect();
        assert_eq!(kinds, vec![Kind::SessionCreated, Kind::Message, Kind::ToolUsed]);
        assert!(service.session_events(SessionId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn title_is_the_first_human_message() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Agent, "agent", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "first", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "second", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title.as_deref(), Some("first"));
    }

    #[tokio::test]
    async fn title_skips_empty_human_messages() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, " ", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "first", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title.as_deref(), Some("first"));
    }

    #[tokio::test]
    async fn title_is_none_without_a_human_message() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Agent, "agent", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title, None);
    }

    #[tokio::test]
    async fn record_creates_a_session_for_an_unknown_client_session_id() {
        let store = Memory::default();

        AppService::new(&store)
            .record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "hi", json!({})))
            .await
            .unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0].payload,
            Payload::SessionCreated { client_session_id, .. } if client_session_id.as_str() == "a"
        ));
        assert!(matches!(&events[1].payload, Payload::Message { content, .. } if content == "hi"));
        assert_eq!(events[1].payload.session(), events[0].payload.session());
    }

    #[tokio::test]
    async fn record_reuses_the_session_of_a_known_client_session_id() {
        let store = Memory::default();
        let service = AppService::new(&store);

        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();

        let events = store.0.lock().unwrap();
        let kinds: Vec<Kind> = events.iter().map(|e| e.payload.kind()).collect();
        assert_eq!(
            kinds,
            vec![Kind::SessionCreated, Kind::ToolUsed, Kind::ToolUsed, Kind::SessionCreated, Kind::ToolUsed]
        );
        assert_eq!(events[1].payload.session(), events[0].payload.session());
        assert_eq!(events[2].payload.session(), events[0].payload.session());
        assert_ne!(events[4].payload.session(), events[0].payload.session());
    }

    #[tokio::test]
    async fn record_gives_the_session_created_event_the_same_source_and_no_later_a_time() {
        let store = Memory::default();
        let source = Source { client: "codex".into(), path: "/work/app".into() };

        AppService::new(&store)
            .record(client_session_id("a"), source.clone(), |src, s| Event::tool_used(src, s, "Bash", json!({})))
            .await
            .unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events[0].source, source);
        assert_eq!(events[1].source, source);
        assert!(events[0].id <= events[1].id);
        assert!(events[0].created_at <= events[1].created_at);
    }

    #[tokio::test]
    async fn the_same_map_kind_can_be_created_in_two_projects_but_not_twice_in_one() {
        let store = Memory::default();
        let service = AppService::new(&store);
        let other = Source { client: "claude".into(), path: "/other".into() };
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.create_map(other, Actor::Human, "decisions", "B").await.unwrap();

        let again = service.create_map(Source::fixture(), Actor::Agent, "decisions", "C").await;

        assert!(again.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn maps_are_the_current_projects_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store);
        let other = Source { client: "claude".into(), path: "/other".into() };
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.create_map(other, Actor::Human, "tasks", "B").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks", "C").await.unwrap();

        let maps = service.maps(&Source::fixture().path).await.unwrap();

        let found: Vec<(&str, &str)> = maps.iter().map(|m| (m.kind.as_str(), m.title.as_str())).collect();
        assert_eq!(found, vec![("decisions", "A"), ("tasks", "C")]);
    }

    #[tokio::test]
    async fn add_node_to_a_missing_map_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();

        let result = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T").await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn blank_kinds_and_titles_are_errors() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q").await.unwrap();

        let map = service.create_map(Source::fixture(), Actor::Human, " ", "B").await;
        let node = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "").await;
        let edge = service.add_edge(Source::fixture(), Actor::Human, "decisions", "Q", "", "Q").await;

        assert!(map.is_err());
        assert!(node.is_err());
        assert!(edge.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_repeated_edge_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O").await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let again = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await;
        let other_kind = service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "blocks", "Q").await;

        assert!(again.is_err());
        assert!(other_kind.is_ok());
    }

    #[tokio::test]
    async fn a_node_title_is_unique_within_its_map_but_not_across_maps() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks", "B").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Why?").await.unwrap();

        let same_map = service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "Why?").await;
        let other_map = service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "Why?").await;

        assert!(same_map.is_err());
        assert!(other_map.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn nodes_are_the_maps_own_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks", "B").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T").await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, "decisions", "option", "O").await.unwrap();
        let map = service.map(&Source::fixture().path, "decisions").await.unwrap();

        let nodes = service.nodes(map.id).await.unwrap();

        let found: Vec<(&str, &str)> = nodes.iter().map(|n| (n.kind.as_str(), n.title.as_str())).collect();
        assert_eq!(found, vec![("question", "Q"), ("option", "O")]);
        assert!(service.map(&Source::fixture().path, "nope").await.is_err());
    }

    #[tokio::test]
    async fn an_edge_links_two_titled_nodes_and_a_missing_title_is_an_error() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O").await.unwrap();

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
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks", "B").await.unwrap();
        for (map, title) in [("decisions", "Q"), ("decisions", "O"), ("tasks", "T"), ("tasks", "U")] {
            service.add_node(Source::fixture(), Actor::Human, map, "n", title).await.unwrap();
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
        let service = AppService::new(&store);
        service.create_map(Source::fixture(), Actor::Human, "decisions", "A").await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, "tasks", "B").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "question", "Q").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "decisions", "option", "O").await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, "tasks", "task", "T").await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, "decisions", "O", "answers", "Q").await.unwrap();

        let summaries = service.map_summaries(&Source::fixture().path).await.unwrap();

        let found: Vec<(&str, usize, usize)> = summaries.iter().map(|s| (s.map.kind.as_str(), s.nodes, s.edges)).collect();
        assert_eq!(found, vec![("decisions", 2, 1), ("tasks", 1, 0)]);
    }

    #[tokio::test]
    async fn create_map_passes_store_errors_through() {
        let result = AppService::new(Broken).create_map(Source::fixture(), Actor::Human, "decisions", "A").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn record_passes_store_errors_through() {
        let result = AppService::new(Broken)
            .record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({})))
            .await;

        assert!(result.is_err());
    }
}
