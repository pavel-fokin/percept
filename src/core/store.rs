use std::error::Error;

use super::{ClientSessionId, Event, SessionId};

pub trait EventStore {
    /// Held while recording; dropping it releases the lock.
    type Lock: Send;

    fn lock(&self) -> impl Future<Output = Result<Self::Lock, Box<dyn Error + Send + Sync>>> + Send;

    fn session(
        &self,
        client_session_id: &ClientSessionId,
    ) -> impl Future<Output = Result<Option<SessionId>, Box<dyn Error + Send + Sync>>> + Send;

    fn all(&self) -> impl Future<Output = Result<Vec<Event>, Box<dyn Error + Send + Sync>>> + Send;

    fn append(
        &self,
        event: &Event,
    ) -> impl Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send;
}
