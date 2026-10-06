//! Application layer: `AppService` runs use cases over the domain.

use std::error::Error;

use crate::core::EventStore;

pub struct AppService {
    store: Box<dyn EventStore>,
}

impl AppService {
    pub fn new(store: Box<dyn EventStore>) -> Self {
        Self { store }
    }

    pub fn event_count(&self) -> Result<usize, Box<dyn Error>> {
        Ok(self.store.all()?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Event;
    use crate::shared::Id;

    struct Fixed(usize);

    impl EventStore for Fixed {
        fn all(&self) -> Result<Vec<Event>, Box<dyn Error>> {
            Ok((0..self.0).map(|_| Event { id: Id::new() }).collect())
        }
    }

    struct Broken;

    impl EventStore for Broken {
        fn all(&self) -> Result<Vec<Event>, Box<dyn Error>> {
            Err("boom".into())
        }
    }

    #[test]
    fn counts_the_stored_events() {
        assert_eq!(AppService::new(Box::new(Fixed(3))).event_count().unwrap(), 3);
    }

    #[test]
    fn passes_store_errors_through() {
        assert!(AppService::new(Box::new(Broken)).event_count().is_err());
    }
}
