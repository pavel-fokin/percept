use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::tools::Workspace;

/// The `list_files` tool: one directory's immediate entries, sorted by
/// name, directories marked with a trailing `/`. Not recursive; use
/// `find_files` to match a pattern across the tree instead.
pub struct ListFiles {
    workspace: Arc<Workspace>,
}

impl ListFiles {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "list_files";

const DESCRIPTION: &str = "List the entries directly inside a directory \
    of the working tree, one name per line, sorted, directories \
    suffixed with `/`. Not recursive - call again on a subdirectory to \
    go deeper, or use find_files to match a pattern across the whole \
    tree.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "path": {"type": "string", "description": "directory to list, relative to the workspace root; defaults to the root"}
  },
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    #[serde(default = "root")]
    path: String,
}

fn root() -> String {
    ".".to_string()
}

impl Tool for ListFiles {
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

        if !resolved.is_dir() {
            return Err(format!("{} is not a directory", args.path).into());
        }

        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&resolved)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type()?.is_dir();
            entries.push(if is_dir { format!("{name}/") } else { name });
        }
        entries.sort();

        Ok(ToolOutput::text(entries.join("\n")))
    }
}

#[cfg(test)]
mod tests;
