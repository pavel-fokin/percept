//! Application layer: `AppService` runs use cases over the domain.

use std::error::Error;

use crate::core::{Event, EventStore, SessionId, SessionKey};

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
    use crate::core::{Actor, Kind};
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
            unimplemented!()
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

        service.record(key("a"), |s| Event::tool_used(s, json!({}))).await.unwrap();
        service.record(key("a"), |s| Event::tool_used(s, json!({}))).await.unwrap();
        service.record(key("b"), |s| Event::tool_used(s, json!({}))).await.unwrap();

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
            .record(key("a"), |s| Event::tool_used(s, json!({})))
            .await;

        assert!(result.is_err());
    }
}
