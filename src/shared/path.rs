use std::path::Path;

/// `path` with `/` between its components whatever the platform -
/// how a path is shown to a reader or a model.
pub fn to_slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
