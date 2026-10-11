//! Finds the git repository a folder belongs to.

use std::path::{Path, PathBuf};

/// The nearest folder at or above `start` that holds a `.git` directory or
/// file, or `None` outside any repository. A worktree's `.git` is a file.
pub async fn repo_root(start: &Path) -> Option<PathBuf> {
    for folder in start.ancestors() {
        if tokio::fs::try_exists(folder.join(".git")).await.unwrap_or(false) {
            return Some(folder.to_path_buf());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "percept-git-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn finds_the_repo_root_itself() {
        let dir = TempDir::new();
        std::fs::create_dir(dir.0.join(".git")).unwrap();
        assert_eq!(repo_root(&dir.0).await, Some(dir.0.clone()));
    }

    #[tokio::test]
    async fn finds_the_root_from_a_nested_folder() {
        let dir = TempDir::new();
        std::fs::create_dir(dir.0.join(".git")).unwrap();
        let nested = dir.0.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(repo_root(&nested).await, Some(dir.0.clone()));
    }

    #[tokio::test]
    async fn counts_a_git_file_as_a_root() {
        let dir = TempDir::new();
        std::fs::write(dir.0.join(".git"), "gitdir: elsewhere").unwrap();
        assert_eq!(repo_root(&dir.0).await, Some(dir.0.clone()));
    }

    // Assumes no `.git` above `std::env::temp_dir()`.
    #[tokio::test]
    async fn reports_none_outside_a_repo() {
        let dir = TempDir::new();
        assert_eq!(repo_root(&dir.0).await, None);
    }
}
