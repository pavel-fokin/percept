use std::error::Error;

use super::EventId;

/// The working tree as it stood before a prompt changed it, named by
/// that prompt. Taken once per turn, before the model is asked, so a
/// turn the user rejects can be put back. The domain asks for "the
/// tree as of this prompt" and nothing about how it is kept.
pub trait Snapshot: Send + Sync {
    /// Records the tree as it stands now, under `prompt`.
    fn take(&self, prompt: EventId) -> Result<(), Box<dyn Error>>;

    /// Puts the tree back as `take(prompt)` saw it.
    fn restore(&self, prompt: EventId) -> Result<(), Box<dyn Error>>;
}
