use std::path::{Component, Path, PathBuf};

use crate::shared::Result;

/// The directory the skills live in.
pub fn library(home: &Path) -> PathBuf {
    home.join(".percept").join("skills")
}

/// The directory of the skill `name` in the library; a name that is not one plain path segment is an error.
pub fn skill_dir(home: &Path, name: &str) -> Result<PathBuf> {
    let mut parts = Path::new(name).components();
    if !matches!((parts.next(), parts.next()), (Some(Component::Normal(_)), None)) {
        return Err(format!("{name:?} is not a skill name").into());
    }

    Ok(library(home).join(name))
}
