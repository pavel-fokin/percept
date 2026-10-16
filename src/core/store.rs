use super::{ClientSessionId, Event, SessionId};
use crate::shared::Result;

pub trait EventStore {
    /// Held while recording; dropping it releases the lock.
    type Lock: Send;

    fn lock(&self) -> impl Future<Output = Result<Self::Lock>> + Send;

    fn session(
        &self,
        client_session_id: &ClientSessionId,
    ) -> impl Future<Output = Result<Option<SessionId>>> + Send;

    fn all(&self) -> impl Future<Output = Result<Vec<Event>>> + Send;

    /// Writes the events in order as one write, so no other append lands inside the batch.
    fn append(
        &self,
        events: &[Event],
    ) -> impl Future<Output = Result<()>> + Send;
}
