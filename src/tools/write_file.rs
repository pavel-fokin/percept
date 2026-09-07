use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::tools::Workspace;

/// The `write_file` tool: writes a file's whole content, creating any
/// missing parent directories and overwriting whatever was there. Marks
/// the path read, so an `edit_file` right after is allowed - the model
/// just learned this file's content by writing it.
pub struct WriteFile {
    workspace: Arc<Workspace>,
}

impl WriteFile {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "write_file";

const DESCRIPTION: &str = "Write a file in the working tree, creating \
    any missing parent directories. Overwrites the whole file if it \
    already exists.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "path": {"type": "string", "description": "path to the file, relative to the workspace root"},
    "content": {"type": "string", "description": "the file's whole content"}
  },
  "required": ["path", "content"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    path: String,
    content: String,
}

impl Tool for WriteFile {
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

        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&resolved, &args.content)?;
        self.workspace.mark_read(&resolved);

        let relative = self.workspace.relative(&resolved);
        Ok(ToolOutput::text(format!(
            "wrote {} bytes to {relative}",
            args.content.len()
        )))
    }
}

#[cfg(test)]
mod tests;
