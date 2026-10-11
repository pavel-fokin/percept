use crate::shared::Id;

/// A named body of understanding in one project, created by a `MapCreated` event.
pub struct Map {
    pub id: MapId,
    pub kind: String,
    pub title: String,
}

pub type MapId = Id<Map>;
