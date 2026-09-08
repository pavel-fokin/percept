use crate::harness::{Policy, Verdict};

/// The tools that change the tree or run a command: the ones a user
/// wants to see before they happen. Reading is routine and runs
/// unasked.
const ASKED: &[&str] = &["write_file", "edit_file", "bash"];

/// The default policy for a coding turn: `write_file`, `edit_file` and
/// `bash` are put to the user, everything else runs.
pub struct AskBeforeWrites;

impl Policy for AskBeforeWrites {
    fn check(&self, tool: &str, _arguments: &str) -> Verdict {
        if ASKED.contains(&tool) {
            Verdict::Ask
        } else {
            Verdict::Allow
        }
    }
}

#[cfg(test)]
mod tests;
