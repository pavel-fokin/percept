use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::tools::{is_binary, Workspace};

/// The `edit_file` tool: replaces `old_string` with `new_string` in a
/// file already read this session. Refuses an edit of a file the model
/// has not read - an edit from memory, not from what is on disk.
pub struct EditFile {
    workspace: Arc<Workspace>,
}

impl EditFile {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "edit_file";

const DESCRIPTION: &str = "Replace old_string with new_string in a file \
    already read this session with read_file. old_string must be \
    non-empty, match the file's text exactly, and occur exactly once, \
    unless replace_all is set, in which case every occurrence is \
    replaced. old_string and new_string must differ. The file must be \
    UTF-8.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "path": {"type": "string", "description": "path to the file, relative to the workspace root"},
    "old_string": {"type": "string", "description": "text to replace, matched exactly"},
    "new_string": {"type": "string", "description": "text to replace it with"},
    "replace_all": {"type": "boolean", "description": "replace every occurrence instead of requiring exactly one; defaults to false"}
  },
  "required": ["path", "old_string", "new_string"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

impl Tool for EditFile {
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

        if !self.workspace.was_read(&resolved) {
            return Err(format!("{} has not been read; call read_file first", args.path).into());
        }
        if args.old_string.is_empty() {
            return Err("old_string is empty".into());
        }
        if args.old_string == args.new_string {
            return Err("old_string and new_string are the same".into());
        }

        let bytes = std::fs::read(&resolved)?;
        if is_binary(&bytes) {
            return Err(format!("{} is binary", args.path).into());
        }
        // Strict, not lossy: a lossy decode written back would rewrite
        // every byte that is not UTF-8, far from the edit.
        let text = String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8", args.path))?;

        let count = text.matches(&args.old_string).count();
        if count == 0 {
            return Err(format!("old_string was not found in {}", args.path).into());
        }
        if !args.replace_all && count > 1 {
            return Err(format!(
                "old_string occurs {count} times in {}; make it unique with more context, or pass replace_all",
                args.path
            )
            .into());
        }

        let replacements = if args.replace_all { count } else { 1 };
        let updated = text.replacen(&args.old_string, &args.new_string, replacements);
        std::fs::write(&resolved, updated)?;

        let relative = self.workspace.relative(&resolved);
        Ok(ToolOutput::text(format!(
            "edited {relative}: {replacements} replacement(s)"
        )))
    }
}

#[cfg(test)]
mod tests;
