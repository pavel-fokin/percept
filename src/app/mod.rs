//! Application layer: `AppService` runs use cases over the domain.

mod app_service;
mod boards;
mod git;
mod maps;
mod projects;
mod sessions;
mod skills;
#[cfg(test)]
mod testing;

pub use app_service::AppService;
pub use boards::BoardContents;
pub use git::source_path;
pub use maps::{CitedMessage, MapDetail, Ref};
pub use projects::Taken;
pub use skills::{library, skill_dir};
