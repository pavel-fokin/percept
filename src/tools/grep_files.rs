use std::sync::Arc;

use regex::Regex;
use serde::Deserialize;

use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::tools::{join_capped, walk};
use crate::workspace::{is_binary, Workspace};

/// Cap on matches returned, so a broad pattern can't flood the model's
/// window. The walk stops at the cap: past it the rest of the tree is
/// read for a count the model only uses as "narrow it".
const MAX_MATCHES: usize = 200;

/// How many characters of a matching line are kept.
const MAX_LINE_CHARS: usize = 200;

/// The `grep_files` tool: a regex search over one file or a directory
/// tree, gitignore-aware, with an optional file-name glob to narrow
/// which files are searched.
pub struct GrepFiles {
    workspace: Arc<Workspace>,
}

impl GrepFiles {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "grep_files";

const DESCRIPTION: &str = "Search text files for lines matching a \
    regex, case-sensitive. `path` narrows the search to one file or a \
    directory (default the whole workspace); `glob` further narrows by \
    file name, e.g. `*.rs`. Gitignored and binary files are skipped; \
    dot-directories such as .percept are searched. \
    Results are `path:line:text`, text clipped to 200 characters. The \
    search stops at 200 matches; narrow the pattern, path, or glob if \
    the result says so.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "pattern": {"type": "string", "description": "a regex, matched against each line"},
    "path": {"type": "string", "description": "a file or directory to search, relative to the workspace root; defaults to the root"},
    "glob": {"type": "string", "description": "a file-name glob filter, e.g. *.rs, matched against the file name only"}
  },
  "required": ["pattern"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    pattern: String,
    #[serde(default = "root")]
    path: String,
    glob: Option<String>,
}

fn root() -> String {
    ".".to_string()
}

impl Tool for GrepFiles {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        let regex = Regex::new(&args.pattern)?;
        let name_glob = args
            .glob
            .map(|glob| globset::Glob::new(&glob).map(|g| g.compile_matcher()))
            .transpose()?;

        let resolved = self.workspace.resolve(&args.path)?;

        let mut matches = Vec::new();
        for entry in walk(&resolved) {
            if matches.len() > MAX_MATCHES {
                break;
            }
            if let Some(glob) = &name_glob {
                if !glob.is_match(entry.file_name()) {
                    continue;
                }
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            if is_binary(&bytes) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let relative = self.workspace.relative(entry.path());
            for (number, line) in text.lines().enumerate() {
                if regex.is_match(line) {
                    let clipped: String = line.chars().take(MAX_LINE_CHARS).collect();
                    matches.push(format!("{relative}:{}:{clipped}", number + 1));
                }
            }
        }

        Ok(ToolOutput::text(join_capped(matches, MAX_MATCHES, |_| {
            format!("[more than {MAX_MATCHES} matches; narrow the pattern or path]")
        })))
    }
}

#[cfg(test)]
mod tests;
