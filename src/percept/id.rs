use std::marker::PhantomData;

use uuid::Uuid;

/// Id is a phantom-typed UUID: T pins it to one entity type, so IDs from
/// different entities can't be mixed up. Per ADR, entity IDs use UUIDv7 -
/// time-ordered, so IDs sort by creation.
///
/// `PhantomData<fn() -> T>` rather than `PhantomData<T>` keeps Id<T>'s
/// Send/Sync/etc. independent of whatever T happens to be - T is a
/// marker, never actually stored.
pub struct Id<T> {
    uuid: Uuid,
    _entity: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub fn new() -> Self {
        Self {
            uuid: Uuid::now_v7(),
            _entity: PhantomData,
        }
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Manual impls, not #[derive], so these never pick up a spurious `T: Trait`
// bound - Id<T> is Clone/Copy/PartialEq/Eq regardless of what T is.
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
