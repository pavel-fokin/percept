use std::sync::Arc;

use regex::Regex;
use serde::Deserialize;

use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::tools::Workspace;

/// How much of a file's start is checked for a NUL byte before it is
/// searched as text.
const BINARY_SNIFF_BYTES: usize = 8192;

/// Cap on matches returned, so a broad pattern can't flood the model's
/// window.
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
    Results are `path:line:text`, text clipped to 200 characters, \
    capped at 200 matches; narrow the pattern, path, or glob if the \
    result says more were cut.";

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
    path: Option<String>,
    glob: Option<String>,
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

        let path = args.path.unwrap_or_else(|| ".".to_string());
        let resolved = self.workspace.resolve(&path)?;

        let mut matches = Vec::new();
        for entry in self.workspace.walk(&resolved) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            if let Some(glob) = &name_glob {
                let name = entry.file_name();
                if !glob.is_match(name) {
                    continue;
                }
            }

            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            if bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let relative = self.workspace.relative(entry.path());

            for (number, line) in text.lines().enumerate() {
                if !regex.is_match(line) {
                    continue;
                }
                let clipped: String = line.chars().take(MAX_LINE_CHARS).collect();
                matches.push(format!("{relative}:{}:{clipped}", number + 1));
            }
        }

        let total = matches.len();
        let truncated = total > MAX_MATCHES;
        matches.truncate(MAX_MATCHES);

        let mut out = matches.join("\n");
        if truncated {
            let remaining = total - MAX_MATCHES;
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!(
                "[{remaining} more matches; narrow the pattern or path]"
            ));
        }

        Ok(ToolOutput::text(out))
    }
}

#[cfg(test)]
mod tests;
