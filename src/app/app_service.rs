use crate::core::{EventStore, SkillStore};
use crate::shared::Result;

pub struct AppService<S: EventStore, M: SkillStore> {
    pub(super) store: S,
    pub(super) skills: M,
}

impl<S: EventStore, M: SkillStore> AppService<S, M> {
    pub fn new(store: S, skills: M) -> Self {
        Self { store, skills }
    }

    pub async fn event_count(&self) -> Result<usize> {
        Ok(self.store.all().await?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Fixed, Skills};

    #[tokio::test]
    async fn counts_the_stored_events() {
        assert_eq!(AppService::new(Fixed(3), Skills).event_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn event_count_passes_store_errors_through() {
        assert!(AppService::new(Broken, Skills).event_count().await.is_err());
    }
}
