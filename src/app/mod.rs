//! Application layer: `AppService` runs use cases over the domain.

use std::error::Error;

use crate::core::{Event, EventStore};

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

    pub async fn record(
        &self,
        payload: serde_json::Value,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.store.append(&Event::new(payload)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use serde_json::json;

    struct Fixed(usize);

    impl EventStore for Fixed {
        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            Ok((0..self.0).map(|_| Event::new(json!({}))).collect())
        }

        async fn append(&self, _: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
    }

    struct Broken;

    impl EventStore for Broken {
        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            Err("boom".into())
        }

        async fn append(&self, _: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            Err("boom".into())
        }
    }

    #[derive(Default)]
    struct Recording(Mutex<Vec<serde_json::Value>>);

    impl EventStore for &Recording {
        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }

        async fn append(&self, event: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.0.lock().unwrap().push(event.payload.clone());
            Ok(())
        }
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
    async fn record_appends_an_event_with_the_payload() {
        let store = Recording::default();
        AppService::new(&store).record(json!({"a": 1})).await.unwrap();
        assert_eq!(*store.0.lock().unwrap(), vec![json!({"a": 1})]);
    }

    #[tokio::test]
    async fn record_passes_store_errors_through() {
        assert!(AppService::new(Broken).record(json!({})).await.is_err());
    }
}
