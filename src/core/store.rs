use super::{ClientSessionId, Event, SessionId};
use crate::shared::Result;

/// The log of events. An operation begins a transaction and runs on it.
pub trait EventLog {
    type Transaction: EventStore + Send + Sync;

    /// Takes the log's lock and reads the log once. Dropping the transaction releases the lock.
    fn begin(&self) -> impl Future<Output = Result<Self::Transaction>> + Send;
}

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
