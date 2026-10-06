//! Domain layer: `Event` and the store trait that persists it.

use std::error::Error;

use serde::{Deserialize, Serialize};

use crate::shared::Id;

pub type EventId = Id<Event>;

#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
}

pub trait EventStore {
    fn all(&self) -> Result<Vec<Event>, Box<dyn Error>>;
}
