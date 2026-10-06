//! Domain layer: `Event` and the store trait that persists it.

use std::error::Error;

use serde::{Deserialize, Serialize};

use crate::shared::Id;

pub type EventId = Id<Event>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Client {
    Claude,
    Codex,
}

#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub client: Client,
    pub payload: serde_json::Value,
}

pub trait EventStore {
    fn all(&self)
        -> impl Future<Output = Result<Vec<Event>, Box<dyn Error + Send + Sync>>> + Send;

    fn append(
        &self,
        event: &Event,
    ) -> impl Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send;
}
