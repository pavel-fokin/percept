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

    fn append(
        &self,
        event: &Event,
    ) -> impl Future<Output = Result<()>> + Send;
}
