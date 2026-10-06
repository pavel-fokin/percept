//! Application layer: `AppService` runs use cases over the domain.

mod git;

use std::error::Error;
use std::path::{Path, PathBuf};

use crate::core::EventStore;

pub struct AppService<S: EventStore> {
    store: S,
}

impl<S: EventStore> AppService<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn event_count(&self) -> Result<usize, Box<dyn Error + Send + Sync>> {
        Ok(self.store.all().await?.len())
    }
}

/// The git repo root containing `dir`, else `dir` itself.
pub async fn source_path(dir: &Path) -> PathBuf {
    git::repo_root(dir).await.unwrap_or_else(|| dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Event;
    use crate::shared::Id;

    struct Fixed(usize);

    impl EventStore for Fixed {
        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            Ok((0..self.0).map(|_| Event { id: Id::new() }).collect())
        }
    }

    struct Broken;

    impl EventStore for Broken {
        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            Err("boom".into())
        }
    }

    #[tokio::test]
    async fn counts_the_stored_events() {
        assert_eq!(AppService::new(Fixed(3)).event_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn passes_store_errors_through() {
        assert!(AppService::new(Broken).event_count().await.is_err());
    }
}
