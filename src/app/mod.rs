//! Application layer: `AppService` runs use cases over the domain.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use crate::core::{Actor, ClientSessionId, Event, EventStore, Map, MapId, Payload, Session, SessionId, Source};

mod git;

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
        let maps = self
            .store
            .all()
            .await?
            .into_iter()
            .filter(|e| e.source.path == project)
            .filter_map(|e| match e.payload {
                Payload::MapCreated { kind, title, .. } => Some(Map { kind, title }),
                _ => None,
            })
            .collect();

        Ok(maps)
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
