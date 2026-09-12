use super::Map;

/// Opens a cognitive map by name - folded from the log for a declared
/// map, walked from the working tree for `code`. Domain-owned
/// the way `EventLog` and `MapRenderer` are, so `ReadMap` opens a map
/// without knowing which source built it.
pub trait MapReader: Send + Sync {
    /// The whole map `name` names. The caller cuts it to a fragment.
    fn read(&self, name: &str) -> Result<Map, Box<dyn std::error::Error>>;
}
