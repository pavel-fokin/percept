use crate::core::{DefinitionStore, EventStore};
use crate::shared::Result;

pub struct AppService<S: EventStore, M: DefinitionStore> {
    pub(super) store: S,
    pub(super) definitions: M,
}

impl<S: EventStore, M: DefinitionStore> AppService<S, M> {
    pub fn new(store: S, definitions: M) -> Self {
        Self { store, definitions }
    }

    pub async fn event_count(&self) -> Result<usize> {
        Ok(self.store.all().await?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Definitions, Fixed};

    #[tokio::test]
    async fn counts_the_stored_events() {
        assert_eq!(AppService::new(Fixed(3), Definitions).event_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn event_count_passes_store_errors_through() {
        assert!(AppService::new(Broken, Definitions).event_count().await.is_err());
    }
}
