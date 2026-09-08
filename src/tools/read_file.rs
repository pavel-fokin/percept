use std::sync::Arc;

use serde::Deserialize;

use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::tools::{is_binary, Workspace};

/// Default number of lines a call returns when the model gives no
/// `limit` - large enough for most files, small enough that a huge one
/// still comes back in a page.
const DEFAULT_LIMIT: usize = 2000;

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
    #[serde(default = "first_line")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn first_line() -> usize {
    1
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
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
        if is_binary(&bytes) {
            return Err(format!("{} is binary", args.path).into());
        }
        let text = String::from_utf8_lossy(&bytes);

        // A model may send 0 for either; both mean "from the start".
        let start = args.offset.max(1) - 1;
        let limit = args.limit.max(1);
        let mut lines = text.lines();
        let window: Vec<&str> = lines.by_ref().skip(start).take(limit).collect();
        let remaining = lines.count();

        let mut out = window.join("\n");
        if remaining > 0 {
            let next = start + window.len() + 1;
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
