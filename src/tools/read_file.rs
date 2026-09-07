use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::tools::Workspace;

/// Default number of lines a call returns when the model gives no
/// `limit` - large enough for most files, small enough that a huge one
/// still comes back in a page.
const DEFAULT_LIMIT: usize = 2000;

/// How much of a file's start is checked for a NUL byte before it is
/// read as text.
const BINARY_SNIFF_BYTES: usize = 8192;

/// The `read_file` tool: returns a window of a file's lines, offset
/// and limit both 1-based counts into the file. When lines remain past
/// the window, the result ends with a trailer naming the next offset,
/// so paging through a large file is one field to change per call.
pub struct ReadFile {
    workspace: Arc<Workspace>,
}

impl ReadFile {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "read_file";

const DESCRIPTION: &str = "Read a text file from the working tree, \
    lines `offset` through `offset + limit - 1` (both 1-based). Omit \
    `offset` and `limit` to read from the start, up to 2000 lines. When \
    more lines remain, the result ends with a line naming the offset to \
    call again with. A binary file is refused.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "path": {"type": "string", "description": "path to the file, relative to the workspace root"},
    "offset": {"type": "integer", "minimum": 1, "description": "first line to return, 1-based; defaults to 1"},
    "limit": {"type": "integer", "minimum": 1, "description": "how many lines to return; defaults to 2000"}
  },
  "required": ["path"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl Tool for ReadFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        let resolved = self.workspace.resolve(&args.path)?;

        let bytes = std::fs::read(&resolved)?;
        if bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) {
            return Err(format!("{} is binary", args.path).into());
        }
        let text = String::from_utf8_lossy(&bytes);

        let offset = args.offset.unwrap_or(1).max(1);
        let limit = args.limit.unwrap_or(DEFAULT_LIMIT).max(1);

        let lines: Vec<&str> = text.lines().collect();
        let start = offset.saturating_sub(1).min(lines.len());
        let end = start.saturating_add(limit).min(lines.len());
        let window = &lines[start..end];

        let mut out = window.join("\n");
        if end < lines.len() {
            let next = end + 1;
            let remaining = lines.len() - end;
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!(
                "[{remaining} more lines; call again with offset {next}]"
            ));
        }

        self.workspace.mark_read(&resolved);
        Ok(ToolOutput::text(out))
    }
}

#[cfg(test)]
mod tests;
