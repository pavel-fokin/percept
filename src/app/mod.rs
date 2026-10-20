//! Application layer: `AppService` runs use cases over the domain.

mod app_service;
mod git;
mod maps;
mod sessions;
#[cfg(test)]
mod testing;

pub use app_service::AppService;
pub use git::source_path;
pub use maps::{CitedMessage, Ref};
