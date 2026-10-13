use std::collections::BTreeMap;

use super::AppService;
use crate::core::{Actor, ClientSessionId, Event, EventStore, Payload, Session, SessionId, Source};
use crate::shared::Result;

impl<S: EventStore> AppService<S> {
    /// Sessions newest first.
    pub async fn sessions(&self) -> Result<Vec<Session>> {
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
                    path: e.source.path.clone(),
                    title: titles.get(session).map(|t| t.to_string()),
                }),
                _ => None,
            })
            .collect();

        Ok(sessions)
    }

    pub async fn session(&self, id: SessionId) -> Result<Option<Session>> {
        Ok(self.sessions().await?.into_iter().find(|s| s.id == id))
    }

    /// The session's events in log order, or `None` if the session does not exist.
    pub async fn session_events(&self, id: SessionId) -> Result<Option<Vec<Event>>> {
        let events: Vec<Event> = self.store.all().await?.into_iter().filter(|e| e.payload.session() == Some(id)).collect();

        let exists = events.iter().any(|e| matches!(e.payload, Payload::SessionCreated { .. }));
        Ok(exists.then_some(events))
    }

    /// Appends the event to the session the client's session id names, creating the session first if it is new.
    pub async fn record(
        &self,
        client_session_id: ClientSessionId,
        source: Source,
        event: impl FnOnce(Source, SessionId) -> Event,
    ) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Memory};
    use crate::core::Kind;

    use serde_json::json;

    fn client_session_id(id: &str) -> ClientSessionId {
        ClientSessionId::new(id.into())
    }

    #[tokio::test]
    async fn passes_store_errors_through() {
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
    async fn path_is_the_source_path_of_the_created_event() {
        let store = Memory::default();
        let service = AppService::new(&store);
        let source = Source { client: "claude".into(), path: "/work/app".into() };
        service.record(client_session_id("a"), source, |src, s| Event::tool_used(src, s, "Bash", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].path, std::path::Path::new("/work/app"));
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
    async fn record_passes_store_errors_through() {
        let result = AppService::new(Broken)
            .record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", json!({})))
            .await;

        assert!(result.is_err());
    }
}
