//! Application layer: `AppService` runs use cases over the domain.

use std::collections::BTreeMap;
use std::error::Error;

use crate::core::{Actor, Event, EventStore, Kind, Session, SessionId, SessionKey};

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
        for e in events.iter().filter(|e| e.kind == Kind::Message && e.actor == Actor::Human) {
            if let Some(content) = e.payload["content"].as_str() {
                titles.entry(e.session).or_insert(content);
            }
        }

        let mut sessions = events
            .iter()
            .filter(|e| e.kind == Kind::SessionCreated)
            .map(|e| {
                let key = e.payload["key"].as_str().ok_or("SessionCreated without a key")?;
                Ok(Session {
                    id: e.session,
                    key: SessionKey::new(key.into()),
                    created_at: e.created_at,
                    title: titles.get(&e.session).map(|t| t.to_string()),
                })
            })
            .collect::<Result<Vec<_>, Box<dyn Error + Send + Sync>>>()?;
        sessions.reverse();

        Ok(sessions)
    }

    pub async fn session(&self, id: SessionId) -> Result<Option<Session>, Box<dyn Error + Send + Sync>> {
        Ok(self.sessions().await?.into_iter().find(|s| s.id == id))
    }

    /// The session's events in log order, or `None` if the session does not exist.
    pub async fn session_events(&self, id: SessionId) -> Result<Option<Vec<Event>>, Box<dyn Error + Send + Sync>> {
        let events: Vec<Event> = self.store.all().await?.into_iter().filter(|e| e.session == id).collect();

        // Every session starts with its SessionCreated event, so no events means no session.
        Ok(if events.is_empty() { None } else { Some(events) })
    }

    /// Appends the event to the session the key names, creating the session first if it is new.
    pub async fn record(
        &self,
        key: SessionKey,
        event: impl FnOnce(SessionId) -> Event,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Held until return, so parallel hooks cannot create the same session twice.
        let _lock = self.store.lock().await?;

        let session = match self.store.session(&key).await? {
            Some(session) => session,
            None => {
                let created = Event::session_created(key);
                self.store.append(&created).await?;
                created.session
            }
        };

        self.store.append(&event(session)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use serde_json::json;

    struct Fixed(usize);

    impl EventStore for Fixed {
        type Lock = ();

        async fn lock(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }

        async fn session(&self, _: &SessionKey) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }

        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            Ok((0..self.0)
                .map(|_| Event::session_started(SessionId::new(), json!({})))
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

        async fn session(&self, _: &SessionKey) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
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

        async fn session(&self, key: &SessionKey) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
            let events = self.0.lock().unwrap();
            Ok(events
                .iter()
                .find(|e| e.kind == Kind::SessionCreated && e.payload["key"] == key.as_str())
                .map(|e| e.session))
        }

        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            let events = self.0.lock().unwrap();
            events.iter().map(|e| Ok(serde_json::from_value(serde_json::to_value(e)?)?)).collect()
        }

        async fn append(&self, event: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            let line = serde_json::to_string(event)?;
            self.0.lock().unwrap().push(serde_json::from_str(&line)?);
            Ok(())
        }
    }

    fn key(key: &str) -> SessionKey {
        SessionKey::new(key.into())
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
        service.record(key("a"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();
        service.record(key("b"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();
        service.record(key("a"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        let events = store.0.lock().unwrap();
        let found: Vec<(SessionId, &str)> = sessions.iter().map(|s| (s.id, s.key.as_str())).collect();
        assert_eq!(found, vec![(events[2].session, "b"), (events[0].session, "a")]);
        assert_eq!(sessions[1].created_at, events[0].created_at);
    }

    #[tokio::test]
    async fn session_finds_one_session_by_id() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(key("a"), |s| Event::message(s, Actor::Human, "hi", json!({}))).await.unwrap();
        service.record(key("b"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();
        let id = store.0.lock().unwrap()[0].session;

        let found = service.session(id).await.unwrap().unwrap();

        assert_eq!(found.id, id);
        assert_eq!(found.key.as_str(), "a");
        assert_eq!(found.title.as_deref(), Some("hi"));
        assert!(service.session(SessionId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn session_events_are_the_sessions_own_in_log_order() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(key("a"), |s| Event::message(s, Actor::Human, "hi", json!({}))).await.unwrap();
        service.record(key("b"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();
        service.record(key("a"), |s| Event::tool_used(s, "Read", json!({}))).await.unwrap();
        let id = store.0.lock().unwrap()[0].session;

        let events = service.session_events(id).await.unwrap().unwrap();

        let kinds: Vec<Kind> = events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![Kind::SessionCreated, Kind::Message, Kind::ToolUsed]);
        assert!(service.session_events(SessionId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn title_is_the_first_human_message() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(key("a"), |s| Event::message(s, Actor::Agent, "agent", json!({}))).await.unwrap();
        service.record(key("a"), |s| Event::message(s, Actor::Human, "first", json!({}))).await.unwrap();
        service.record(key("a"), |s| Event::message(s, Actor::Human, "second", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title.as_deref(), Some("first"));
    }

    #[tokio::test]
    async fn title_is_none_without_a_human_message() {
        let store = Memory::default();
        let service = AppService::new(&store);
        service.record(key("a"), |s| Event::message(s, Actor::Agent, "agent", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title, None);
    }

    #[tokio::test]
    async fn record_creates_a_session_for_an_unknown_key() {
        let store = Memory::default();

        AppService::new(&store)
            .record(key("a"), |s| Event::message(s, Actor::Human, "hi", json!({})))
            .await
            .unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, Kind::SessionCreated);
        assert_eq!(events[0].payload, json!({"key": "a"}));
        assert_eq!(events[1].kind, Kind::Message);
        assert_eq!(events[1].payload, json!({"content": "hi"}));
        assert_eq!(events[1].session, events[0].session);
    }

    #[tokio::test]
    async fn record_reuses_the_session_of_a_known_key() {
        let store = Memory::default();
        let service = AppService::new(&store);

        service.record(key("a"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();
        service.record(key("a"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();
        service.record(key("b"), |s| Event::tool_used(s, "Bash", json!({}))).await.unwrap();

        let events = store.0.lock().unwrap();
        let kinds: Vec<Kind> = events.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                Kind::SessionCreated,
                Kind::ToolUsed,
                Kind::ToolUsed,
                Kind::SessionCreated,
                Kind::ToolUsed
            ]
        );
        assert_eq!(events[1].session, events[0].session);
        assert_eq!(events[2].session, events[0].session);
        assert_ne!(events[4].session, events[0].session);
    }

    #[tokio::test]
    async fn record_passes_store_errors_through() {
        let result = AppService::new(Broken)
            .record(key("a"), |s| Event::tool_used(s, "Bash", json!({})))
            .await;

        assert!(result.is_err());
    }
}
