//! Native file tools over a working tree - `read_file`, `list_files`,
//! `find_files`, `grep_files` - each a `percept::Tool` the way
//! `src/store`'s event tools are, but reading the filesystem instead
//! of the event log. `Workspace` is the one place a path the model
//! gave becomes a real path, shared by every tool here.

mod find_files;
mod grep_files;
mod list_files;
mod read_file;
mod workspace;

pub use find_files::FindFiles;
pub use grep_files::GrepFiles;
pub use list_files::ListFiles;
pub use read_file::ReadFile;
pub use workspace::Workspace;
