use super::Map;

/// Where a map's rendering lands - domain-owned the way `EventLog` and
/// `Model` are, so `App` and the CLI write verbs can ask for a map to
/// be rendered without knowing it becomes a file.
pub trait MapRenderer: Send + Sync {
    /// Rerenders `map` wherever this renderer keeps it, replacing
    /// whatever was there before.
    fn render(&self, map: &Map) -> Result<(), Box<dyn std::error::Error>>;
}
