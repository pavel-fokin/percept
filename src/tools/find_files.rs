use std::sync::Arc;

use globset::GlobBuilder;
use ignore::WalkBuilder;
use serde::Deserialize;

use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::tools::Workspace;

/// Cap on matches returned, so a broad pattern can't flood the
/// model's window with the whole tree.
const MAX_MATCHES: usize = 500;

/// The `find_files` tool: walks the working tree the way `.gitignore`
/// does, matching each file's root-relative path against a glob, and
/// returns the matches sorted, one per line. Narrow the glob when the
/// result is cut short.
pub struct FindFiles {
    workspace: Arc<Workspace>,
}

impl FindFiles {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "find_files";

const DESCRIPTION: &str = "Find files under the working tree whose \
    path matches a glob, e.g. `src/**/*.rs`. Gitignored and hidden \
    files are skipped, matching `.gitignore`. Results are sorted paths \
    relative to the workspace root, capped at 500; narrow the glob if \
    the result says more were cut.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "pattern": {"type": "string", "description": "a glob relative to the workspace root, e.g. src/**/*.rs"}
  },
  "required": ["pattern"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    pattern: String,
}

impl Tool for FindFiles {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        let glob = GlobBuilder::new(&args.pattern)
            .literal_separator(true)
            .build()?
            .compile_matcher();

        let root = self.workspace.root();
        let mut matches = Vec::new();
        for entry in WalkBuilder::new(root).require_git(false).build() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
            if glob.is_match(relative) {
                matches.push(self.workspace.relative(entry.path()));
            }
        }
        matches.sort();

        let total = matches.len();
        let truncated = total > MAX_MATCHES;
        matches.truncate(MAX_MATCHES);

        let mut out = matches.join("\n");
        if truncated {
            let remaining = total - MAX_MATCHES;
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{remaining} more matches; narrow the pattern]"));
        }

        Ok(ToolOutput::text(out))
    }
}

#[cfg(test)]
mod tests;
