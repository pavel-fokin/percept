use std::sync::Arc;

use globset::GlobBuilder;
use serde::Deserialize;

use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::tools::{join_capped, walk};
use crate::workspace::Workspace;

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

        let mut matches: Vec<String> = walk(self.workspace.root())
            .map(|entry| self.workspace.relative(entry.path()))
            .filter(|relative| glob.is_match(relative))
            .collect();
        matches.sort();

        Ok(ToolOutput::text(join_capped(matches, MAX_MATCHES, |n| {
            format!("[{n} more matches; narrow the pattern]")
        })))
    }
}

#[cfg(test)]
mod tests;
