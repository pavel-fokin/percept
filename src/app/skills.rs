use std::path::{Path, PathBuf};

/// The directory the skills live in.
pub fn library(home: &Path) -> PathBuf {
    home.join(".percept").join("skills")
}
