//! Domain layer: `Event` and the store trait that persists it.

use std::error::Error;

use serde::{Deserialize, Serialize};

use crate::shared::Id;

pub type EventId = Id<Event>;

#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub client: String,
    pub payload: serde_json::Value,
}

impl Event {
    pub fn new(client: String, payload: serde_json::Value) -> Self {
        Self {
            id: EventId::new(),
            client,
            payload,
        }
    }
}

pub trait EventStore {
    fn all(&self)
        -> impl Future<Output = Result<Vec<Event>, Box<dyn Error + Send + Sync>>> + Send;

    fn append(
        &self,
        event: &Event,
    ) -> impl Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send;
}
