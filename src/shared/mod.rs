//! Foundation layer: code every layer shares, such as `Id<T>`.

use std::cmp::Ordering;
use std::fmt;
use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// A UUIDv7 that identifies one entity of type `T`.
pub struct Id<T> {
    uuid: Uuid,
    // `fn() -> T` keeps `Id<T>` Send, Sync and Copy whatever `T` is.
    entity: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    #[cfg_attr(not(test), expect(dead_code, reason = "nothing records an Event yet"))]
    pub fn new() -> Self {
        Self {
            uuid: Uuid::now_v7(),
            entity: PhantomData,
        }
    }
}

// Derives would require `T` itself to implement each trait.

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl<T> Eq for Id<T> {}

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.uuid.cmp(&other.uuid)
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.uuid)
    }
}

impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.uuid.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            uuid: Uuid::deserialize(deserializer)?,
            entity: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Entity;

    #[test]
    fn new_ids_are_distinct_and_ordered() {
        let first = Id::<Entity>::new();
        let second = Id::<Entity>::new();

        assert_ne!(first, second);
        assert!(first < second);
    }

    #[test]
    fn id_round_trips_through_json_as_a_uuid_string() {
        let id = Id::<Entity>::new();

        let json = serde_json::to_string(&id).unwrap();
        let back: Id<Entity> = serde_json::from_str(&json).unwrap();

        assert_eq!(json, format!("\"{}\"", id.uuid));
        assert_eq!(back, id);
    }
}
