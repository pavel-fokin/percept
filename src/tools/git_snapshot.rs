use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::percept::{EventId, Snapshot};

/// Where a turn's snapshots are kept: a ref per prompt, outside every
/// branch, so `git log` and `git status` never show them and a branch
/// switch never touches them.
const REF_PREFIX: &str = "refs/percept/snapshots/";

/// The `Snapshot` a git checkout keeps: before each prompt, a commit of
/// the whole tree - tracked, modified, and untracked files alike,
/// ignored ones aside - under `refs/percept/snapshots/<prompt>`,
/// parented on `HEAD` so `git diff <ref>` shows what the turn changed.
/// The commit is built through a scratch index, so the user's own
/// index and branch are left as they were. Only the newest snapshot
/// is kept: `App` restores the last turn and no earlier one, and a
/// ref that is never restored would pin every untracked file it saw,
/// secrets included, for as long as the repository lives. `restore`
/// puts the tree and the index back to that commit, removes files the
/// turn created, and unstages everything, so what was uncommitted
/// before the turn is uncommitted again - though no longer staged.
pub struct GitSnapshot {
    checkout: PathBuf,
    /// The repository's git dir, where the scratch index goes.
    git_dir: PathBuf,
}

impl GitSnapshot {
    /// Opens the repository `checkout` is in. Errs when it is not one,
    /// or git is not installed - at startup, so a coding session never
    /// finds out at its first prompt.
    pub fn open(checkout: &Path) -> Result<Self, Box<dyn Error>> {
        let mut snapshot = Self {
            checkout: checkout.to_path_buf(),
            git_dir: PathBuf::new(),
        };
        snapshot.git_dir = PathBuf::from(
            snapshot
                .git(&["rev-parse", "--absolute-git-dir"], None)
                .map_err(|err| format!("{}: {err}", checkout.display()))?,
        );
        Ok(snapshot)
    }

    /// Runs one git command at the checkout and returns its stdout, or
    /// its stderr as the error.
    fn git(&self, args: &[&str], index: Option<&Path>) -> Result<String, Box<dyn Error>> {
        let mut command = Command::new("git");
        command
            .args(args)
            .current_dir(&self.checkout)
            .env("GIT_AUTHOR_NAME", "percept")
            .env("GIT_AUTHOR_EMAIL", "percept@localhost")
            .env("GIT_COMMITTER_NAME", "percept")
            .env("GIT_COMMITTER_EMAIL", "percept@localhost");
        if let Some(index) = index {
            command.env("GIT_INDEX_FILE", index);
        }
        let output = command.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("git {}: {stderr}", args[0]).into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// `HEAD` as a commit id, or `None` in a repository with no commit
    /// yet.
    fn head(&self) -> Option<String> {
        self.git(&["rev-parse", "--verify", "--quiet", "HEAD"], None)
            .ok()
            .filter(|head| !head.is_empty())
    }
}

fn ref_for(prompt: EventId) -> String {
    format!("{REF_PREFIX}{}", prompt.as_uuid())
}

impl Snapshot for GitSnapshot {
    fn take(&self, prompt: EventId) -> Result<(), Box<dyn Error>> {
        // A scratch index in the repository's own git dir - never the
        // user's index, which `git add -A` would otherwise stage into.
        // Named by pid, so two sessions in one checkout do not share it.
        let index = self
            .git_dir
            .join(format!("percept-snapshot-index-{}", std::process::id()));
        let head = self.head();
        let result = (|| {
            match &head {
                Some(head) => self.git(&["read-tree", head], Some(&index))?,
                None => self.git(&["read-tree", "--empty"], Some(&index))?,
            };
            self.git(&["add", "-A"], Some(&index))?;
            let tree = self.git(&["write-tree"], Some(&index))?;
            let message = format!("percept snapshot before {}", prompt.as_uuid());
            let mut args = vec!["commit-tree", tree.as_str(), "-m", message.as_str()];
            if let Some(head) = &head {
                args.extend(["-p", head.as_str()]);
            }
            let commit = self.git(&args, None)?;
            let new_ref = ref_for(prompt);
            self.git(&["update-ref", &new_ref, &commit], None)?;
            for old_ref in self
                .git(&["for-each-ref", "--format=%(refname)", REF_PREFIX], None)?
                .lines()
                .filter(|old_ref| *old_ref != new_ref)
            {
                self.git(&["update-ref", "-d", old_ref], None)?;
            }
            Ok(())
        })();
        let _ = std::fs::remove_file(&index);
        result
    }

    fn restore(&self, prompt: EventId) -> Result<(), Box<dyn Error>> {
        let snapshot = ref_for(prompt);
        self.git(&["rev-parse", "--verify", "--quiet", &snapshot], None)
            .map_err(|_| format!("no snapshot for prompt {}", prompt.as_uuid()))?;
        // Tree and index to the snapshot; then anything untracked is a
        // file the turn created, so it goes, ignored files aside; then
        // the index back to HEAD, leaving the tree as restored.
        self.git(&["read-tree", "-u", "--reset", &snapshot], None)?;
        self.git(&["clean", "-fdq"], None)?;
        match self.head() {
            Some(_) => self.git(&["reset", "-q"], None)?,
            None => self.git(&["read-tree", "--empty"], None)?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests;
