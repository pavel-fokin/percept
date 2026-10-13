use crate::core::{EventStore, SchemaStore};
use crate::shared::Result;

pub struct AppService<S: EventStore, M: SchemaStore> {
    pub(super) store: S,
    pub(super) schemas: M,
}

impl<S: EventStore, M: SchemaStore> AppService<S, M> {
    pub fn new(store: S, schemas: M) -> Self {
        Self { store, schemas }
    }

    pub async fn event_count(&self) -> Result<usize> {
        Ok(self.store.all().await?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Fixed, Schemas};

    #[tokio::test]
    async fn counts_the_stored_events() {
        assert_eq!(AppService::new(Fixed(3), Schemas).event_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn event_count_passes_store_errors_through() {
        assert!(AppService::new(Broken, Schemas).event_count().await.is_err());
    }
}
