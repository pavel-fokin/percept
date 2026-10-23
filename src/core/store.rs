use super::{ClientSessionId, Event, SessionId};
use crate::shared::Result;

/// One transaction's view of the log. `all` and `session` read the snapshot taken at `begin`;
/// an `append` is visible to the reads that follow it in the same transaction.
pub trait EventStore {
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
