use std::sync::Mutex;

use serde_json::json;

use crate::core::{ClientSessionId, Event, EventStore, Payload, SessionId, Source};
use crate::shared::Result;

pub(super) struct Fixed(pub(super) usize);

impl EventStore for Fixed {
    type Lock = ();

    async fn lock(&self) -> Result<()> {
        unimplemented!()
    }

    async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
        unimplemented!()
    }

    async fn all(&self) -> Result<Vec<Event>> {
        Ok((0..self.0)
            .map(|_| Event::session_started(Source::fixture(), SessionId::new(), json!({})))
            .collect())
    }

    async fn append(&self, _: &Event) -> Result<()> {
        unimplemented!()
    }
}

pub(super) struct Broken;

impl EventStore for Broken {
    type Lock = ();

    async fn lock(&self) -> Result<()> {
        Err("boom".into())
    }

    async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
        Err("boom".into())
    }

    async fn all(&self) -> Result<Vec<Event>> {
        Err("boom".into())
    }

    async fn append(&self, _: &Event) -> Result<()> {
        Err("boom".into())
    }
}

#[derive(Default)]
pub(super) struct Memory(pub(super) Mutex<Vec<Event>>);

impl EventStore for &Memory {
    type Lock = ();

    async fn lock(&self) -> Result<()> {
        Ok(())
    }

    async fn session(&self, client_session_id: &ClientSessionId) -> Result<Option<SessionId>> {
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

    async fn all(&self) -> Result<Vec<Event>> {
        let events = self.0.lock().unwrap();
        Ok(events.clone())
    }

    async fn append(&self, event: &Event) -> Result<()> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
}
